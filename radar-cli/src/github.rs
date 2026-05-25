use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use radar_core::diff::DiffChange;
use radar_core::models::Severity;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::env;

use crate::render::{BlastRadiusEntry, BlastRadiusResponse};

// Marker embedded in every drift comment so we can find and update it.
const COMMENT_MARKER: &str = "<!-- radar-monitor-report -->";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GithubContext {
    pub token: String,
    pub owner: String,
    pub repo: String,
    pub pr_number: u64,
}

impl GithubContext {
    /// Detect GitHub context from CI environment variables.
    /// Returns None if not running in a GitHub Actions PR context.
    pub fn from_env() -> Option<Self> {
        let token = env::var("GITHUB_TOKEN").ok()?;

        let repo_env = env::var("GITHUB_REPOSITORY").ok()?;
        let mut parts = repo_env.splitn(2, '/');
        let owner = parts.next()?.to_string();
        let repo = parts.next()?.to_string();

        // Detect PR number from multiple sources, in priority order.
        let pr_number = detect_pr_number()?;

        Some(GithubContext {
            token,
            owner,
            repo,
            pr_number,
        })
    }
}

/// Try to detect a PR number from the environment, in order:
/// 1. `GITHUB_PR_NUMBER` (explicit override)
/// 2. `GITHUB_REF` like `refs/pull/42/merge`
/// 3. `GITHUB_EVENT_PATH` JSON file, `.pull_request.number`
fn detect_pr_number() -> Option<u64> {
    // 1. Explicit env var
    if let Ok(val) = env::var("GITHUB_PR_NUMBER") {
        if let Ok(n) = val.trim().parse::<u64>() {
            return Some(n);
        }
    }

    // 2. Parse refs/pull/{n}/merge from GITHUB_REF
    if let Ok(github_ref) = env::var("GITHUB_REF") {
        if let Some(n) = parse_pr_from_ref(&github_ref) {
            return Some(n);
        }
    }

    // 3. Parse GITHUB_EVENT_PATH JSON
    if let Ok(event_path) = env::var("GITHUB_EVENT_PATH") {
        if let Ok(contents) = std::fs::read_to_string(&event_path) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(n) = value
                    .get("pull_request")
                    .and_then(|pr| pr.get("number"))
                    .and_then(|n| n.as_u64())
                {
                    return Some(n);
                }
            }
        }
    }

    None
}

fn parse_pr_from_ref(github_ref: &str) -> Option<u64> {
    // refs/pull/42/merge
    let stripped = github_ref.strip_prefix("refs/pull/")?;
    let num_str = stripped.split('/').next()?;
    num_str.parse::<u64>().ok()
}

// ---------------------------------------------------------------------------
// Comment building
// ---------------------------------------------------------------------------

/// Lightweight summary of a generated test suite — used in PR comments (H-5).
#[derive(Deserialize, Clone)]
pub struct TestSuiteSummary {
    pub id: String,
    pub collection_name: String,
    pub test_count: u64,
}

/// Build the Markdown comment body from the diff results.
/// Pass `test_suites = &[]` when no suites are available (H-5).
pub fn build_comment_with_suites(
    changes: &[DiffChange],
    from_ref: &str,
    to_ref: &str,
    blast_radius: Option<&BlastRadiusResponse>,
    verdict: &str,
    fail_mode: &str,
    test_suites: &[TestSuiteSummary],
) -> String {
    if changes.is_empty() {
        return format!(
            "{marker}\n## \u{2705} Radar Monitor \u{2014} No API Changes\n\nNo schema changes detected between `{from_ref}` and `{to_ref}`.\n",
            marker = COMMENT_MARKER,
        );
    }

    let breaking = changes
        .iter()
        .filter(|c| c.severity == Severity::Breaking)
        .count();
    let risky = changes
        .iter()
        .filter(|c| c.severity == Severity::NonBreakingRisky)
        .count();
    let safe = changes
        .iter()
        .filter(|c| c.severity == Severity::Safe)
        .count();

    let mut rows = String::new();
    for change in changes {
        let emoji = severity_emoji(&change.severity);
        let label = severity_label(&change.severity);
        rows.push_str(&format!(
            "| {emoji} **{label}** | `{}` | `{}` |\n",
            change.path,
            change.kind.as_str(),
        ));
    }

    let evidence_section = blast_radius
        .map(|br| render_evidence_section(&br.entries))
        .unwrap_or_default();

    let consumer_count = blast_radius.map(|br| br.entries.len()).unwrap_or(0);
    let verdict_section = render_policy_verdict_section(verdict, fail_mode, consumer_count);

    let suite_section = if test_suites.is_empty() {
        String::new()
    } else {
        let mut s = String::from("### Generated Test Suites\n\n");
        for ts in test_suites {
            s.push_str(&format!(
                "- **{}** — {} test(s) · suite ID `{}`\n",
                ts.collection_name, ts.test_count, ts.id
            ));
        }
        s.push('\n');
        s
    };

    format!(
        "{marker}\n\
## \u{1f50d} Radar Monitor \u{2014} API Contract Check\n\n\
Comparing `{from_ref}` \u{2192} `{to_ref}`\n\n\
| Severity | Path | Change |\n\
|---|---|---|\n\
{rows}\n\
---\n\
**{breaking} breaking** \u{00b7} **{risky} risky** \u{00b7} **{safe} safe**\n\n\
{evidence_section}\
{suite_section}\
{verdict_section}\
<sub>Generated by [Radar Monitor](https://github.com) v0.1.0</sub>\n",
        marker = COMMENT_MARKER,
    )
}

// ---------------------------------------------------------------------------
// E-4: Evidence table renderer
// ---------------------------------------------------------------------------

fn confidence_order(c: &str) -> u8 {
    match c {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    }
}

fn confidence_label(order: u8) -> &'static str {
    match order {
        0 => "high",
        1 => "medium",
        _ => "low",
    }
}

fn format_relative(ts: &str) -> String {
    if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
        let days = Utc::now().signed_duration_since(dt).num_days();
        match days {
            0 => "today".to_string(),
            1 => "1 day ago".to_string(),
            d if d < 30 => format!("{d} days ago"),
            d if d < 60 => "about 1 month ago".to_string(),
            d => format!("about {} months ago", d / 30),
        }
    } else {
        ts.get(..10).unwrap_or(ts).to_string()
    }
}

fn render_evidence_section(entries: &[BlastRadiusEntry]) -> String {
    // Flatten all evidence items, tagging each with consumer name and confidence order.
    // Row: (conf_order, consumer, source, operation, field_path, last_seen)
    let mut flat: Vec<(u8, String, String, String, String, String)> = Vec::new();

    for entry in entries {
        let conf_order = confidence_order(&entry.confidence);
        for ev in &entry.evidence {
            let operation = ev
                .operation
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("—")
                .to_string();
            let field_path = ev
                .field_path
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("—")
                .to_string();
            let last_seen = if ev.kind == "runtime_usage" {
                ev.recorded_at
                    .as_deref()
                    .map(format_relative)
                    .unwrap_or_else(|| "—".to_string())
            } else {
                "(static)".to_string()
            };
            flat.push((
                conf_order,
                entry.consumer.name.clone(),
                ev.kind.clone(),
                operation,
                field_path,
                last_seen,
            ));
        }
    }

    if flat.is_empty() {
        return String::new();
    }

    flat.sort_by_key(|(ord, _, _, _, _, _)| *ord);

    let total = flat.len();
    let mut out = String::from("### Evidence\n\n");
    out.push_str("| Consumer | Source | Operation | Field Path | Confidence | Last Seen |\n");
    out.push_str("|---|---|---|---|---|---|\n");

    for (conf_order, consumer, source, operation, field_path, last_seen) in flat.iter().take(10) {
        let conf = confidence_label(*conf_order);
        out.push_str(&format!(
            "| {consumer} | {source} | {operation} | {field_path} | {conf} | {last_seen} |\n"
        ));
    }

    if total > 10 {
        out.push_str(&format!(
            "\n_{} more evidence record(s) not shown._\n",
            total - 10
        ));
    }

    out.push('\n');
    out
}

// ---------------------------------------------------------------------------
// E-4: Policy verdict section renderer
// ---------------------------------------------------------------------------

fn render_policy_verdict_section(verdict: &str, fail_mode: &str, consumer_count: usize) -> String {
    let badge = match verdict {
        "block" => "**BLOCKED**",
        "warn" => "**WARNED**",
        "overridden" => "**OVERRIDDEN**",
        _ => "**PASSED**",
    };

    let mut out = String::from("### Policy Verdict\n\n");
    out.push_str(&format!("> {badge} \u{00b7} fail_mode: {fail_mode}\n\n"));

    match verdict {
        "block" => {
            out.push_str(&format!(
                "{consumer_count} consumer(s) affected. At least 1 high-confidence evidence record present.\n\
                 To override: add the `drift-ack` label to this PR and re-run CI.\n"
            ));
        }
        "warn" => {
            out.push_str(&format!(
                "{consumer_count} consumer(s) affected. Build not blocked \u{2014} review recommended.\n"
            ));
        }
        "overridden" => {
            out.push_str(&format!(
                "Override active. Build allowed despite {consumer_count} affected consumer(s).\n"
            ));
        }
        _ => {}
    }

    out.push('\n');
    out
}

fn severity_emoji(severity: &Severity) -> &'static str {
    match severity {
        Severity::Breaking => "\u{1f534}",        // 🔴
        Severity::NonBreakingRisky => "\u{1f7e1}", // 🟡
        Severity::Safe => "\u{1f7e2}",             // 🟢
    }
}

fn severity_label(severity: &Severity) -> &'static str {
    match severity {
        Severity::Breaking => "BREAKING",
        Severity::NonBreakingRisky => "Risky",
        Severity::Safe => "Safe",
    }
}

// ---------------------------------------------------------------------------
// GitHub API interaction
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CommentResponse {
    id: u64,
    body: String,
}

#[derive(Serialize)]
struct CommentBody<'a> {
    body: &'a str,
}

#[derive(Deserialize)]
struct CreatedComment {
    html_url: String,
}

/// Post or update (idempotent) the drift comment on the PR.
/// Returns the comment URL on success.
pub async fn post_or_update_comment(ctx: &GithubContext, body: &str) -> Result<String> {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(
        USER_AGENT,
        HeaderValue::from_static("radar-monitor/0.1"),
    );

    let client = reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .context("failed to build HTTP client")?;

    let auth_value = format!("Bearer {}", ctx.token);
    let auth_header =
        HeaderValue::from_str(&auth_value).context("invalid GITHUB_TOKEN value")?;

    // List existing comments on the PR.
    let list_url = format!(
        "https://api.github.com/repos/{}/{}/issues/{}/comments?per_page=100",
        ctx.owner, ctx.repo, ctx.pr_number
    );

    let list_resp = client
        .get(&list_url)
        .header(AUTHORIZATION, auth_header.clone())
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .context("failed to list PR comments")?;

    if !list_resp.status().is_success() {
        let status = list_resp.status();
        let text = list_resp
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable>".into());
        bail!("GitHub API error listing comments: {} — {}", status, text);
    }

    let comments: Vec<CommentResponse> = list_resp
        .json()
        .await
        .context("failed to parse comment list response")?;

    let existing = comments.iter().find(|c| c.body.starts_with(COMMENT_MARKER));

    let response_body = if let Some(existing_comment) = existing {
        // PATCH to update the existing comment.
        let patch_url = format!(
            "https://api.github.com/repos/{}/{}/issues/comments/{}",
            ctx.owner, ctx.repo, existing_comment.id
        );
        let resp = client
            .patch(&patch_url)
            .header(AUTHORIZATION, auth_header)
            .header(ACCEPT, "application/vnd.github+json")
            .json(&CommentBody { body })
            .send()
            .await
            .context("failed to update PR comment")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
            bail!("GitHub API error updating comment: {} — {}", status, text);
        }

        resp
    } else {
        // POST a new comment.
        let post_url = format!(
            "https://api.github.com/repos/{}/{}/issues/{}/comments",
            ctx.owner, ctx.repo, ctx.pr_number
        );
        let resp = client
            .post(&post_url)
            .header(AUTHORIZATION, auth_header)
            .header(ACCEPT, "application/vnd.github+json")
            .json(&CommentBody { body })
            .send()
            .await
            .context("failed to post PR comment")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
            bail!("GitHub API error posting comment: {} — {}", status, text);
        }

        resp
    };

    let created: CreatedComment = response_body
        .json()
        .await
        .context("failed to parse comment response")?;

    Ok(created.html_url)
}

// ---------------------------------------------------------------------------
// D-2: Label check
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LabelItem {
    name: String,
}

/// Return true if the PR has a label with the given exact name.
/// Silently returns false on any network/parse error.
pub async fn pr_has_label(ctx: &GithubContext, label: &str) -> bool {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(USER_AGENT, HeaderValue::from_static("radar-monitor/0.1"));

    let client = match reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let auth_value = format!("Bearer {}", ctx.token);
    let auth_header = match HeaderValue::from_str(&auth_value) {
        Ok(h) => h,
        Err(_) => return false,
    };

    let url = format!(
        "https://api.github.com/repos/{}/{}/issues/{}/labels",
        ctx.owner, ctx.repo, ctx.pr_number
    );

    let resp = match client
        .get(&url)
        .header(AUTHORIZATION, auth_header)
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    if !resp.status().is_success() {
        return false;
    }

    let labels: Vec<LabelItem> = match resp.json().await {
        Ok(l) => l,
        Err(_) => return false,
    };

    labels.iter().any(|l| l.name == label)
}

// ---------------------------------------------------------------------------
// D-3: GitHub Release posting
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CreateReleaseBody<'a> {
    tag_name: &'a str,
    name: &'a str,
    body: &'a str,
    draft: bool,
    prerelease: bool,
}

#[derive(Deserialize)]
struct ReleaseResponse {
    html_url: String,
}

/// Create a GitHub Release tagged `tag_name` with the given body.
/// Returns the release URL on success.
pub async fn post_release(
    ctx: &GithubContext,
    tag_name: &str,
    title: &str,
    body: &str,
) -> Result<String> {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(USER_AGENT, HeaderValue::from_static("radar-monitor/0.1"));

    let client = reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .context("failed to build HTTP client")?;

    let auth_value = format!("Bearer {}", ctx.token);
    let auth_header =
        HeaderValue::from_str(&auth_value).context("invalid GITHUB_TOKEN value")?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases",
        ctx.owner, ctx.repo
    );

    let resp = client
        .post(&url)
        .header(AUTHORIZATION, auth_header)
        .header(ACCEPT, "application/vnd.github+json")
        .json(&CreateReleaseBody {
            tag_name,
            name: title,
            body,
            draft: false,
            prerelease: false,
        })
        .send()
        .await
        .context("failed to create GitHub Release")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("GitHub API error creating release: {} — {}", status, text);
    }

    let release: ReleaseResponse = resp
        .json()
        .await
        .context("failed to parse release response")?;

    Ok(release.html_url)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{BlastRadiusEntry, BlastRadiusResponse, ConsumerInfo, EvidenceItem};
    use radar_core::{
        diff::DiffChange,
        models::{ChangeKind, Severity},
    };

    fn sample_changes() -> Vec<DiffChange> {
        vec![
            DiffChange {
                path: "GET /users".into(),
                kind: ChangeKind::OperationRemoved,
                severity: Severity::Breaking,
                description: None,
            },
            DiffChange {
                path: "POST /users".into(),
                kind: ChangeKind::OperationAdded,
                severity: Severity::Safe,
                description: None,
            },
        ]
    }

    fn make_ev(kind: &str, op: &str, field: Option<&str>) -> EvidenceItem {
        EvidenceItem {
            kind: kind.to_string(),
            operation: Some(op.to_string()),
            field_path: field.map(|s| s.to_string()),
            recorded_at: if kind == "runtime_usage" {
                Some("2026-05-23T10:00:00Z".to_string())
            } else {
                None
            },
        }
    }

    fn make_entry(name: &str, confidence: &str, evidence: Vec<EvidenceItem>) -> BlastRadiusEntry {
        BlastRadiusEntry {
            consumer: ConsumerInfo {
                name: name.to_string(),
                owner_team: "team-a".to_string(),
                contact: "team@example.com".to_string(),
            },
            confidence: confidence.to_string(),
            last_seen: "2026-05-23T10:00:00Z".to_string(),
            has_runtime_usage: confidence != "low",
            has_call_site: true,
            evidence,
        }
    }

    fn make_blast_radius(entries: Vec<BlastRadiusEntry>) -> BlastRadiusResponse {
        BlastRadiusResponse { entries }
    }

    // --- existing tests, updated signature ---

    #[test]
    fn comment_contains_marker() {
        let body = build_comment_with_suites(&sample_changes(), "abc", "def", None, "pass", "closed", &[]);
        assert!(body.starts_with(COMMENT_MARKER));
    }

    #[test]
    fn empty_changes_produces_success_message() {
        let body = build_comment_with_suites(&[], "abc", "def", None, "pass", "closed", &[]);
        assert!(body.contains("No API Changes"));
    }

    #[test]
    fn comment_contains_breaking_emoji() {
        let body = build_comment_with_suites(&sample_changes(), "abc", "def", None, "block", "closed", &[]);
        assert!(body.contains("\u{1f534}")); // 🔴
    }

    #[test]
    fn parse_pr_from_ref_extracts_number() {
        assert_eq!(parse_pr_from_ref("refs/pull/42/merge"), Some(42));
        assert_eq!(parse_pr_from_ref("refs/heads/main"), None);
        assert_eq!(parse_pr_from_ref("refs/pull/123/head"), Some(123));
    }

    #[test]
    fn empty_changes_comment_starts_with_marker() {
        let body = build_comment_with_suites(&[], "sha1", "sha2", None, "pass", "closed", &[]);
        assert!(body.starts_with(COMMENT_MARKER));
    }

    #[test]
    fn non_breaking_risky_gets_yellow_emoji() {
        let changes = vec![DiffChange {
            path: "GET /users/{id} \u{2192} param.filter".into(),
            kind: ChangeKind::RequiredChanged,
            severity: Severity::NonBreakingRisky,
            description: None,
        }];
        let body = build_comment_with_suites(&changes, "abc", "def", None, "warn", "warn", &[]);
        assert!(body.contains("\u{1f7e1}")); // 🟡
    }

    // --- E-4: evidence section tests ---

    #[test]
    fn evidence_section_empty_when_no_entries() {
        let section = render_evidence_section(&[]);
        assert!(section.is_empty());
    }

    #[test]
    fn evidence_section_empty_when_entries_have_no_evidence() {
        let entries = vec![make_entry("billing-svc", "high", vec![])];
        let section = render_evidence_section(&entries);
        assert!(section.is_empty());
    }

    #[test]
    fn evidence_section_sorts_high_confidence_first() {
        let entries = vec![
            make_entry(
                "low-svc",
                "low",
                vec![make_ev("call_site", "GET /users/{id}", Some("phone"))],
            ),
            make_entry(
                "high-svc",
                "high",
                vec![make_ev("runtime_usage", "GET /users/{id}", Some("phone"))],
            ),
        ];
        let section = render_evidence_section(&entries);
        let high_pos = section.find("high-svc").unwrap();
        let low_pos = section.find("low-svc").unwrap();
        assert!(high_pos < low_pos, "high confidence row must precede low confidence row");
    }

    #[test]
    fn evidence_section_truncates_at_10_with_footer() {
        let evs: Vec<EvidenceItem> = (0..11)
            .map(|i| make_ev("call_site", &format!("GET /path/{i}"), None))
            .collect();
        let entries = vec![make_entry("svc", "low", evs)];
        let section = render_evidence_section(&entries);
        assert!(
            section.contains("1 more evidence record(s) not shown"),
            "truncation footer missing: {section}"
        );
    }

    #[test]
    fn evidence_section_call_site_shows_static() {
        let entries = vec![make_entry(
            "svc",
            "low",
            vec![make_ev("call_site", "GET /x", Some("field"))],
        )];
        let section = render_evidence_section(&entries);
        assert!(section.contains("(static)"));
    }

    #[test]
    fn evidence_section_runtime_usage_shows_relative_time() {
        let entries = vec![make_entry(
            "svc",
            "high",
            vec![make_ev("runtime_usage", "GET /x", Some("field"))],
        )];
        let section = render_evidence_section(&entries);
        // Timestamp is 2026-05-23; current date is also ~2026-05-25, so "2 days ago" or "today"
        assert!(!section.contains("(static)"));
        assert!(section.contains("ago") || section.contains("today"));
    }

    // --- E-4: policy verdict section tests ---

    #[test]
    fn verdict_blocked_shows_blocked_badge_and_override_hint() {
        let section = render_policy_verdict_section("block", "closed", 2);
        assert!(section.contains("BLOCKED"), "badge missing");
        assert!(section.contains("drift-ack"), "override hint missing");
        assert!(section.contains("fail_mode: closed"));
    }

    #[test]
    fn verdict_warn_shows_warned_badge() {
        let section = render_policy_verdict_section("warn", "warn", 1);
        assert!(section.contains("WARNED"));
        assert!(section.contains("fail_mode: warn"));
    }

    #[test]
    fn verdict_pass_shows_passed_badge() {
        let section = render_policy_verdict_section("pass", "open", 0);
        assert!(section.contains("PASSED"));
    }

    #[test]
    fn verdict_overridden_shows_overridden_badge() {
        let section = render_policy_verdict_section("overridden", "closed", 3);
        assert!(section.contains("OVERRIDDEN"));
    }

    // --- E-4: build_comment integration ---

    #[test]
    fn build_comment_includes_evidence_and_verdict_sections() {
        let br = make_blast_radius(vec![make_entry(
            "billing-svc",
            "high",
            vec![make_ev("runtime_usage", "GET /users/{id}", Some("response.user.phone"))],
        )]);
        let body = build_comment_with_suites(&sample_changes(), "abc", "def", Some(&br), "block", "closed", &[]);
        assert!(body.contains("### Evidence"), "evidence section missing");
        assert!(body.contains("### Policy Verdict"), "verdict section missing");
        assert!(body.contains("BLOCKED"));
        assert!(body.contains("billing-svc"));
    }

    #[test]
    fn build_comment_without_blast_radius_still_shows_verdict() {
        let body = build_comment_with_suites(&sample_changes(), "abc", "def", None, "block", "closed", &[]);
        assert!(body.contains("### Policy Verdict"));
        assert!(!body.contains("### Evidence"), "evidence section must be absent");
    }

    #[test]
    fn build_comment_with_test_suites_includes_suite_section() {
        let suites = vec![TestSuiteSummary {
            id: "suite-abc123".into(),
            collection_name: "Contract Compliance Tests".into(),
            test_count: 6,
        }];
        let body = build_comment_with_suites(&sample_changes(), "abc", "def", None, "pass", "open", &suites);
        assert!(body.contains("### Generated Test Suites"), "test suite section missing");
        assert!(body.contains("Contract Compliance Tests"));
        assert!(body.contains("6 test(s)"));
        assert!(body.contains("suite-abc123"));
    }
}
