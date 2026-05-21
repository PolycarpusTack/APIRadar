use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::claude;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChangeRow {
    path: String,
    kind: String,
    severity: String,
}

#[derive(Deserialize)]
struct DiffDetail {
    id: String,
    from_git_ref: String,
    to_git_ref: String,
    changes: Vec<ChangeRow>,
}

#[derive(Deserialize)]
pub struct BlastEntry {
    pub consumer: ConsumerInfo,
    pub confidence: String,
    pub last_seen: String,
}

#[derive(Deserialize)]
pub struct ConsumerInfo {
    pub name: String,
    pub owner_team: String,
    pub contact: String,
}

#[derive(Deserialize)]
struct BlastRadius {
    #[serde(default)]
    entries: Vec<BlastEntry>,
}

// ── Public entry point ──────────────────────────────────────────────────────

pub async fn run(
    api_url: &str,
    diff_id: &str,
    release_notes: bool,
    migration_guide: bool,
    post_github_release: bool,
    out: Option<&Path>,
    token: Option<&str>,
) -> Result<()> {
    let client = Client::new();

    let diff_url = format!("{api_url}/v1/diffs/{diff_id}");
    let mut req = client.get(&diff_url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let diff: DiffDetail = req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch diff: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("diff API error: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("failed to parse diff response: {e}"))?;

    let blast_url = format!("{api_url}/v1/diffs/{diff_id}/blast-radius");
    let mut blast_req = client.get(&blast_url);
    if let Some(t) = token {
        blast_req = blast_req.header("Authorization", format!("Bearer {t}"));
    }
    let blast: BlastRadius = blast_req
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch blast radius: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("blast-radius API error: {e}"))?
        .json()
        .await
        .unwrap_or(BlastRadius { entries: vec![] });

    if release_notes || migration_guide {
        // Group changes by concept (derived from path prefix).
        let groups = group_by_concept(&diff.changes);

        // Single AI call → per-concept title + prose.
        let narratives = if !groups.is_empty() {
            let prompt = build_concept_prompt(&diff.to_git_ref, &groups);
            match crate::ai_provider::complete(&prompt, 2048).await {
                Some(text) => parse_narratives(&text),
                None => BTreeMap::new(),
            }
        } else {
            BTreeMap::new()
        };

        // Per-consumer migration guides when --migration-guide is set.
        let per_consumer: Vec<Option<String>> = if migration_guide {
            let breaking_summary = breaking_changes_summary(&diff.changes);
            let mut result = Vec::new();
            for entry in &blast.entries {
                let n = claude::generate_consumer_narrative(
                    &entry.consumer.name,
                    &entry.consumer.owner_team,
                    &breaking_summary,
                )
                .await;
                result.push(n);
            }
            result
        } else {
            blast.entries.iter().map(|_| None).collect()
        };

        let notes = build_release_notes(&diff, &groups, &narratives, &blast, &per_consumer, migration_guide);

        if post_github_release {
            match crate::github::GithubContext::from_env() {
                Some(ctx) => {
                    let tag = format!("drift-{}", &diff.id[..8]);
                    let title = format!(
                        "API diff {to} — {n} breaking change(s)",
                        to = diff.to_git_ref,
                        n = diff.changes.iter().filter(|c| c.severity == "breaking").count()
                    );
                    match crate::github::post_release(&ctx, &tag, &title, &notes).await {
                        Ok(url) => println!("GitHub Release created: {url}"),
                        Err(e) => eprintln!("Warning: failed to post GitHub Release: {e}"),
                    }
                }
                None => eprintln!(
                    "Warning: --post-github-release set but GITHUB_TOKEN or repo context not found. Skipping."
                ),
            }
        }

        let store_url = format!("{api_url}/v1/diffs/{diff_id}/release-notes");
        let mut store_req = client
            .post(&store_url)
            .header("content-type", "application/json")
            .body(format!(r#"{{"content":{}}}"#, serde_json::to_string(&notes).unwrap()));
        if let Some(t) = token {
            store_req = store_req.header("Authorization", format!("Bearer {t}"));
        }
        match store_req.send().await {
            Ok(resp) if resp.status().is_success() => {
                eprintln!("Release notes stored in Radar dashboard.");
            }
            Ok(resp) => eprintln!("Warning: failed to store release notes (HTTP {})", resp.status()),
            Err(e) => eprintln!("Warning: could not reach API to store release notes: {e}"),
        }

        match out {
            Some(path) => {
                std::fs::write(path, &notes)
                    .map_err(|e| anyhow::anyhow!("failed to write output file: {e}"))?;
                println!("Release notes written to {}", path.display());
            }
            None => print!("{notes}"),
        }
    } else {
        let breaking = diff.changes.iter().filter(|c| c.severity == "breaking").count();
        let consumers_affected = blast.entries.len();
        println!("Diff {diff_id}: {breaking} breaking change(s), {consumers_affected} consumer(s) affected.");
    }

    Ok(())
}

// ── Concept grouping ────────────────────────────────────────────────────────

/// Groups changes by the first meaningful path segment (concept name).
/// BTreeMap gives stable alphabetical ordering.
fn group_by_concept(changes: &[ChangeRow]) -> BTreeMap<String, Vec<&ChangeRow>> {
    let mut map: BTreeMap<String, Vec<&ChangeRow>> = BTreeMap::new();
    for change in changes {
        map.entry(extract_concept(&change.path)).or_default().push(change);
    }
    map
}

/// Derives a concept name from a change path.
///
/// Handles three path formats:
///   "GET /products/{id}"       → "Products"
///   "/v1/programs/{id}/version" → "Programs"
///   "user.phone"               → "User"
fn extract_concept(path: &str) -> String {
    // Strip HTTP method prefix: "GET /products/{id}" → "/products/{id}"
    let path = if let Some(idx) = path.find(' ') {
        path[idx + 1..].trim()
    } else {
        path.trim()
    };

    // Dot notation (field paths): "user.phone" → "User"
    if !path.contains('/') && path.contains('.') {
        return title_case(path.split('.').next().unwrap_or("general"));
    }

    // URL path: skip leading slash, common version/namespace prefixes, and path parameters
    let skip = ["v1", "v2", "v3", "v4", "api", "rest", "public", "internal"];
    for segment in path.trim_start_matches('/').split('/') {
        if segment.starts_with('{') && segment.ends_with('}') {
            continue;
        }
        if !segment.is_empty() && !skip.contains(&segment.to_lowercase().as_str()) {
            return title_case(segment);
        }
    }

    "General".to_string()
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Deduplicated, sorted endpoint paths for one concept's table cell.
fn concept_calls(changes: &[&ChangeRow]) -> String {
    let mut seen = std::collections::BTreeSet::new();
    for c in changes {
        seen.insert(c.path.clone());
    }
    seen.into_iter().collect::<Vec<_>>().join(", ")
}

// ── AI prompt + response ────────────────────────────────────────────────────

fn build_concept_prompt(version: &str, groups: &BTreeMap<String, Vec<&ChangeRow>>) -> String {
    let mut concept_block = String::new();
    for (concept, changes) in groups {
        concept_block.push_str(&format!("- **{concept}**:\n"));
        for c in changes {
            let marker = if c.severity == "breaking" { " — BREAKING" } else { "" };
            concept_block.push_str(&format!("  - `{}` ({}{})\n", c.path, c.kind, marker));
        }
    }

    format!(
        r#"You are a technical writer producing API release notes for a media technology platform.
Write release notes for API version "{version}" in this professional style.

STYLE EXAMPLES (follow these closely):
- "The storyArcTitle field is now correctly included in YAML export responses. Previously the field was omitted when using the export endpoint. No changes are required on the consumer side."
- "The startDate parameter is now required when creating a Program. Consumers must include this field in all POST /programs requests or a 422 validation error is returned."

TASK: For each concept group below write:
1. A short, specific title (8–12 words). Prefix with "Breaking: " if any change in the group is BREAKING.
2. Two to four sentences of inline prose: what changed → business context → how consumers are affected → migration action if breaking.

CONCEPT GROUPS:
{concept_block}
Return ONLY valid JSON — no markdown fences, no text outside the JSON:
{{"groups":[{{"concept":"ConceptName","title":"Short descriptive title","prose":"2-4 sentence description."}}]}}"#
    )
}

/// Parses the AI JSON response into concept → (title, prose).
fn parse_narratives(text: &str) -> BTreeMap<String, (String, String)> {
    let start = match text.find('{') {
        Some(i) => i,
        None => return BTreeMap::new(),
    };
    let end = match text.rfind('}') {
        Some(i) => i + 1,
        None => return BTreeMap::new(),
    };

    let mut result = BTreeMap::new();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text[start..end]) {
        if let Some(groups) = val["groups"].as_array() {
            for g in groups {
                let concept = g["concept"].as_str().unwrap_or("").to_string();
                let title   = g["title"].as_str().unwrap_or("").to_string();
                let prose   = g["prose"].as_str().unwrap_or("").to_string();
                if !concept.is_empty() {
                    result.insert(concept, (title, prose));
                }
            }
        }
    }
    result
}

// ── Document assembly ───────────────────────────────────────────────────────

fn build_release_notes(
    diff: &DiffDetail,
    groups: &BTreeMap<String, Vec<&ChangeRow>>,
    narratives: &BTreeMap<String, (String, String)>,
    blast: &BlastRadius,
    per_consumer: &[Option<String>],
    include_migration_guide: bool,
) -> String {
    let version = &diff.to_git_ref;
    let today = chrono::Utc::now().format("%Y-%m-%d");
    let mut md = String::new();

    md.push_str("# Release notes\n\n");
    md.push_str(&format!("## {version}\n\n"));
    md.push_str(&format!(
        "> Generated {today} · diff `{}` → `{version}`\n\n",
        diff.from_git_ref
    ));

    if groups.is_empty() {
        md.push_str("_No changes detected in this diff._\n\n");
    } else {
        for (concept, changes) in groups {
            let (title, prose) = narratives
                .get(concept)
                .cloned()
                .unwrap_or_else(|| (concept.clone(), String::new()));

            // H3 title — fall back to concept name if AI wasn't available
            md.push_str(&format!(
                "### {}\n\n",
                if title.is_empty() { concept.as_str() } else { title.as_str() }
            ));

            // Prose
            if !prose.is_empty() {
                md.push_str(&format!("{prose}\n\n"));
            }

            // Concept | Calls table
            md.push_str("| Concept | Calls |\n|---|---|\n");
            md.push_str(&format!("| {concept} | {} |\n\n", concept_calls(changes)));
        }
    }

    // Optional per-consumer migration guide appendix
    if include_migration_guide && !blast.entries.is_empty() {
        md.push_str("---\n\n## Consumer Migration Checklist\n\n");
        for (entry, narrative) in blast
            .entries
            .iter()
            .zip(per_consumer.iter().chain(std::iter::repeat(&None)))
        {
            md.push_str(&format!(
                "### {name}\n\
                 - **Team**: {team}\n\
                 - **Contact**: {contact}\n\
                 - **Confidence**: {confidence}\n\
                 - **Last seen**: {last_seen}\n\n",
                name       = entry.consumer.name,
                team       = entry.consumer.owner_team,
                contact    = entry.consumer.contact,
                confidence = entry.confidence,
                last_seen  = entry.last_seen,
            ));

            match narrative {
                Some(text) => md.push_str(&format!("{text}\n\n")),
                None => md.push_str(
                    "_Migration guide unavailable — configure an AI provider and pass `--migration-guide`._\n\n",
                ),
            }

            md.push_str(
                "- [ ] Review changes above\n\
                 - [ ] Update integration\n\
                 - [ ] Test against new API version\n\n",
            );
        }
    }

    md.push_str("---\n_Generated by radar-cli._\n");
    md
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn breaking_changes_summary(changes: &[ChangeRow]) -> String {
    changes
        .iter()
        .filter(|c| c.severity == "breaking")
        .map(|c| format!("- [{}] {} ({})", c.severity, c.path, c.kind))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_diff(changes: Vec<ChangeRow>) -> DiffDetail {
        DiffDetail {
            id: "diff-1".into(),
            from_git_ref: "2025r11".into(),
            to_git_ref: "2025r12".into(),
            changes,
        }
    }

    fn change(path: &str, kind: &str, severity: &str) -> ChangeRow {
        ChangeRow { path: path.into(), kind: kind.into(), severity: severity.into() }
    }

    // ── extract_concept ──────────────────────────────────────────────────────

    #[test]
    fn concept_from_url_with_method() {
        assert_eq!(extract_concept("GET /products/{contentId}"), "Products");
        assert_eq!(extract_concept("POST /programs"), "Programs");
        assert_eq!(extract_concept("PUT /series/{id}"), "Series");
    }

    #[test]
    fn concept_from_versioned_url() {
        assert_eq!(extract_concept("/v1/programs/{id}"), "Programs");
        assert_eq!(extract_concept("GET /api/v2/products"), "Products");
    }

    #[test]
    fn concept_from_dot_notation() {
        assert_eq!(extract_concept("user.phone"), "User");
        assert_eq!(extract_concept("product.price.currency"), "Product");
    }

    #[test]
    fn concept_falls_back_to_general() {
        assert_eq!(extract_concept("/v1/{id}"), "General");
        assert_eq!(extract_concept(""), "General");
    }

    // ── group_by_concept ─────────────────────────────────────────────────────

    #[test]
    fn groups_by_first_segment() {
        let changes = vec![
            change("GET /products/{id}", "field_removed", "breaking"),
            change("PUT /products/{id}", "field_added", "non_breaking"),
            change("POST /programs", "required_field_added", "breaking"),
        ];
        let groups = group_by_concept(&changes);
        assert!(groups.contains_key("Products"));
        assert!(groups.contains_key("Programs"));
        assert_eq!(groups["Products"].len(), 2);
        assert_eq!(groups["Programs"].len(), 1);
    }

    // ── build_release_notes ───────────────────────────────────────────────────

    #[test]
    fn output_contains_version_heading() {
        let diff = make_diff(vec![change("GET /products/{id}", "field_removed", "breaking")]);
        let groups = group_by_concept(&diff.changes);
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&diff, &groups, &BTreeMap::new(), &blast, &[], false);
        assert!(notes.contains("## 2025r12"));
    }

    #[test]
    fn output_contains_concept_table() {
        let diff = make_diff(vec![change("GET /products/{id}", "field_removed", "breaking")]);
        let groups = group_by_concept(&diff.changes);
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&diff, &groups, &BTreeMap::new(), &blast, &[], false);
        assert!(notes.contains("| Concept | Calls |"));
        assert!(notes.contains("| Products |"));
    }

    #[test]
    fn ai_title_and_prose_appear_in_output() {
        let diff = make_diff(vec![change("GET /products/{id}", "field_removed", "breaking")]);
        let groups = group_by_concept(&diff.changes);
        let mut narratives = BTreeMap::new();
        narratives.insert(
            "Products".to_string(),
            ("Breaking: price removed from Products".to_string(), "The price field has been removed.".to_string()),
        );
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&diff, &groups, &narratives, &blast, &[], false);
        assert!(notes.contains("Breaking: price removed from Products"));
        assert!(notes.contains("The price field has been removed."));
    }

    #[test]
    fn no_migration_section_without_flag() {
        let diff = make_diff(vec![change("GET /products/{id}", "field_removed", "breaking")]);
        let groups = group_by_concept(&diff.changes);
        let blast = BlastRadius {
            entries: vec![BlastEntry {
                consumer: ConsumerInfo {
                    name: "checkout-svc".into(),
                    owner_team: "Payments".into(),
                    contact: "pay@example.com".into(),
                },
                confidence: "high".into(),
                last_seen: "2025-05-18T00:00:00Z".into(),
            }],
        };
        let notes = build_release_notes(&diff, &groups, &BTreeMap::new(), &blast, &[None], false);
        assert!(!notes.contains("Consumer Migration Checklist"));
    }

    #[test]
    fn migration_section_present_with_flag() {
        let diff = make_diff(vec![change("GET /products/{id}", "field_removed", "breaking")]);
        let groups = group_by_concept(&diff.changes);
        let blast = BlastRadius {
            entries: vec![BlastEntry {
                consumer: ConsumerInfo {
                    name: "checkout-svc".into(),
                    owner_team: "Payments".into(),
                    contact: "pay@example.com".into(),
                },
                confidence: "high".into(),
                last_seen: "2025-05-18T00:00:00Z".into(),
            }],
        };
        let notes = build_release_notes(
            &diff,
            &groups,
            &BTreeMap::new(),
            &blast,
            &[Some("Update your GET /products calls.".into())],
            true,
        );
        assert!(notes.contains("Consumer Migration Checklist"));
        assert!(notes.contains("checkout-svc"));
        assert!(notes.contains("Update your GET /products calls."));
    }

    // ── parse_narratives ─────────────────────────────────────────────────────

    #[test]
    fn parses_valid_json_response() {
        let raw = r#"{"groups":[{"concept":"Products","title":"Fix for price field","prose":"The price field was removed."}]}"#;
        let map = parse_narratives(raw);
        assert_eq!(map["Products"].0, "Fix for price field");
        assert_eq!(map["Products"].1, "The price field was removed.");
    }

    #[test]
    fn tolerates_prose_wrapper_around_json() {
        let raw = r#"Here are the release notes: {"groups":[{"concept":"Programs","title":"Breaking change","prose":"StartDate is now required."}]} End."#;
        let map = parse_narratives(raw);
        assert!(map.contains_key("Programs"));
    }

    #[test]
    fn returns_empty_map_on_invalid_json() {
        let map = parse_narratives("not json at all");
        assert!(map.is_empty());
    }
}
