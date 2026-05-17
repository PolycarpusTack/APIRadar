use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;

use crate::claude;

// ── API response types ──────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
struct DiffSummary {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    from_git_ref: String,
    #[allow(dead_code)]
    to_git_ref: String,
    #[allow(dead_code)]
    pr_url: Option<String>,
    #[allow(dead_code)]
    created_at: String,
    #[allow(dead_code)]
    breaking_count: u64,
}

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
    out: Option<&Path>,
    token: Option<&str>,
) -> Result<()> {
    let client = Client::new();

    // 1. Fetch diff details.
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

    // 2. Fetch blast radius.
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

    if release_notes {
        // 3. Optionally call Claude for a narrative.
        let changes_summary = changes_to_summary(&diff.changes);
        let narrative = claude::generate_narrative(&changes_summary).await;

        let notes = build_release_notes(&diff, &blast, narrative.as_deref());

        match out {
            Some(path) => {
                std::fs::write(path, &notes)
                    .map_err(|e| anyhow::anyhow!("failed to write output file: {e}"))?;
                println!("Release notes written to {}", path.display());
            }
            None => print!("{notes}"),
        }
    } else {
        // Short summary mode.
        let breaking = diff
            .changes
            .iter()
            .filter(|c| c.severity == "breaking")
            .count();
        let consumers_affected = blast.entries.len();
        println!(
            "Diff {diff_id}: {breaking} breaking change(s), {consumers_affected} consumer(s) affected."
        );
    }

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn changes_to_summary(changes: &[ChangeRow]) -> String {
    changes
        .iter()
        .map(|c| format!("- [{}] {} ({})", c.severity, c.path, c.kind))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_release_notes(diff: &DiffDetail, blast: &BlastRadius, narrative: Option<&str>) -> String {
    let today = chrono::Utc::now().format("%Y-%m-%d");

    let mut md = String::new();

    // ── Header ────────────────────────────────────────────────────────────
    md.push_str(&format!(
        "# Release Notes\n\n\
         | Field        | Value |\n\
         |---|---|\n\
         | Diff ID      | `{id}` |\n\
         | From ref     | `{from}` |\n\
         | To ref       | `{to}` |\n\
         | Generated    | {today} |\n\n",
        id = diff.id,
        from = diff.from_git_ref,
        to = diff.to_git_ref,
    ));

    // ── Breaking Changes ──────────────────────────────────────────────────
    md.push_str("## Breaking Changes\n\n");

    let narrative_block = match narrative {
        Some(text) => format!("> {text}\n\n"),
        None => "> [narrative unavailable — set ANTHROPIC_API_KEY]\n\n".to_string(),
    };
    md.push_str(&narrative_block);

    let breaking: Vec<&ChangeRow> = diff
        .changes
        .iter()
        .filter(|c| c.severity == "breaking")
        .collect();

    if breaking.is_empty() {
        md.push_str("_No breaking changes._\n\n");
    } else {
        md.push_str("| Path | Kind |\n|---|---|\n");
        for c in &breaking {
            md.push_str(&format!("| `{}` | {} |\n", c.path, c.kind));
        }
        md.push('\n');
    }

    // ── New Capabilities ─────────────────────────────────────────────────
    md.push_str("## New Capabilities\n\n");

    let added: Vec<&ChangeRow> = diff
        .changes
        .iter()
        .filter(|c| c.severity != "breaking" && c.kind.contains("added"))
        .collect();

    if added.is_empty() {
        md.push_str("_No new capabilities._\n\n");
    } else {
        md.push_str("| Path | Kind |\n|---|---|\n");
        for c in &added {
            md.push_str(&format!("| `{}` | {} |\n", c.path, c.kind));
        }
        md.push('\n');
    }

    // ── Deprecations ─────────────────────────────────────────────────────
    md.push_str("## Deprecations\n\n");

    let deprecated: Vec<&ChangeRow> = diff
        .changes
        .iter()
        .filter(|c| c.kind.contains("deprecated"))
        .collect();

    if deprecated.is_empty() {
        md.push_str("_No deprecations._\n\n");
    } else {
        md.push_str("| Path | Kind |\n|---|---|\n");
        for c in &deprecated {
            md.push_str(&format!("| `{}` | {} |\n", c.path, c.kind));
        }
        md.push('\n');
    }

    // ── Per-Consumer Migration Checklist ──────────────────────────────────
    md.push_str("## Per-Consumer Migration Checklist\n\n");

    if blast.entries.is_empty() {
        md.push_str("_No consumers registered._\n\n");
    } else {
        for entry in &blast.entries {
            md.push_str(&format!(
                "### {name}\n\
                 - **Team**: {team}\n\
                 - **Contact**: {contact}\n\
                 - **Confidence**: {confidence}\n\
                 - **Last seen**: {last_seen}\n\n\
                 - [ ] Review breaking changes above\n\
                 - [ ] Update integration\n\
                 - [ ] Test against new API version\n\n",
                name = entry.consumer.name,
                team = entry.consumer.owner_team,
                contact = entry.consumer.contact,
                confidence = entry.confidence,
                last_seen = entry.last_seen,
            ));
        }
    }

    // ── Footer ────────────────────────────────────────────────────────────
    md.push_str("---\n_Generated by drift-cli._\n");

    md
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_diff() -> DiffDetail {
        DiffDetail {
            id: "diff-1".into(),
            from_git_ref: "abc123".into(),
            to_git_ref: "def456".into(),
            changes: vec![
                ChangeRow {
                    path: "GET /users".into(),
                    kind: "operation_removed".into(),
                    severity: "breaking".into(),
                },
                ChangeRow {
                    path: "POST /users".into(),
                    kind: "operation_added".into(),
                    severity: "safe".into(),
                },
            ],
        }
    }

    #[test]
    fn release_notes_contain_header() {
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&sample_diff(), &blast, None);
        assert!(notes.contains("Release Notes"));
        assert!(notes.contains("abc123"));
    }

    #[test]
    fn release_notes_contain_breaking_section() {
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&sample_diff(), &blast, None);
        assert!(notes.contains("Breaking Changes"));
        assert!(notes.contains("GET /users"));
    }

    #[test]
    fn release_notes_placeholder_when_no_api_key() {
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&sample_diff(), &blast, None);
        assert!(notes.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn release_notes_embed_narrative_when_provided() {
        let blast = BlastRadius { entries: vec![] };
        let notes = build_release_notes(&sample_diff(), &blast, Some("Custom narrative here."));
        assert!(notes.contains("Custom narrative here."));
    }
}
