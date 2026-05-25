use colored::Colorize;
use radar_core::{
    diff::DiffChange,
    models::Severity,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Blast-radius response types (deserialized from the API)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
pub struct ConsumerInfo {
    pub name: String,
    pub owner_team: String,
    pub contact: String,
}

#[derive(Deserialize, Clone)]
pub struct EvidenceItem {
    pub kind: String,
    pub operation: Option<String>,
    pub field_path: Option<String>,
    pub recorded_at: Option<String>,
    // last_seen_at (call_site) omitted: serde ignores unknown JSON fields; we show "(static)" in the PR comment
}

#[derive(Deserialize, Clone)]
pub struct BlastRadiusEntry {
    pub consumer: ConsumerInfo,
    pub confidence: String,
    pub last_seen: String,
    pub has_runtime_usage: bool,
    pub has_call_site: bool,
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,
}

#[derive(Deserialize)]
pub struct BlastRadiusResponse {
    pub entries: Vec<BlastRadiusEntry>,
}

/// Machine-readable summary written to `--summary-file` for GitHub Action output parsing.
#[derive(Serialize)]
pub struct CheckSummary {
    pub diff_id: Option<String>,
    pub breaking_count: usize,
    pub affected_consumer_count: usize,
    pub policy_verdict: String,
    pub dashboard_url: Option<String>,
}

/// Write a `CheckSummary` as pretty-printed JSON to the given path.
pub fn write_summary(path: &std::path::Path, summary: &CheckSummary) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(summary)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Print the check results to stdout.
/// If `use_color` is false, suppress ANSI codes (also respects NO_COLOR env var).
pub fn print_table(changes: &[DiffChange], use_color: bool) {
    if !use_color {
        colored::control::set_override(false);
    }

    let title = "drift check — API Contract Radar Monitor".bold();
    println!("{title}");
    println!("{}", "═".repeat(44));

    if changes.is_empty() {
        println!("  {} No changes detected.", "✓".green());
        return;
    }

    for change in changes {
        let (badge, path_colored) = match change.severity {
            Severity::Breaking => (
                "  BREAKING".red().bold(),
                change.path.red().to_string(),
            ),
            Severity::NonBreakingRisky => (
                "     RISKY".yellow().bold(),
                change.path.yellow().to_string(),
            ),
            Severity::Safe => (
                "        ok".cyan(),
                change.path.normal().to_string(),
            ),
        };
        println!("{badge}   {path_colored:<45}  {}", change.kind.as_str());
    }

    println!();
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
    print!("  ");
    if breaking > 0 {
        print!("{} breaking", breaking.to_string().red().bold());
    }
    if risky > 0 {
        if breaking > 0 {
            print!(" · ");
        }
        print!("{} risky", risky.to_string().yellow());
    }
    if safe > 0 {
        if breaking > 0 || risky > 0 {
            print!(" · ");
        }
        print!("{} safe", safe.to_string().cyan());
    }
    println!();
}

/// Print JSON output to stdout.
pub fn print_json(changes: &[DiffChange]) {
    println!(
        "{}",
        serde_json::to_string_pretty(changes).unwrap_or_else(|_| "[]".into())
    );
}

/// Print the blast-radius report to stdout.
pub fn print_blast_radius(br: &BlastRadiusResponse, use_color: bool) {
    if !use_color {
        colored::control::set_override(false);
    }

    println!();
    let title = "Blast Radius — Affected Consumers".bold();
    println!("{title}");
    println!("{}", "─".repeat(44));

    if br.entries.is_empty() {
        println!("  No registered consumers affected.");
        return;
    }

    // Column header
    println!(
        "  {:<20}  {:<12}  {:<8}  {:<8}  {:<30}  {:<20}  {}",
        "Consumer".underline(),
        "Confidence".underline(),
        "Runtime".underline(),
        "CallSite".underline(),
        "Last Seen".underline(),
        "Team".underline(),
        "Contact".underline(),
    );

    for entry in &br.entries {
        let confidence_display = match entry.confidence.as_str() {
            "high" => "high".red().bold().to_string(),
            "medium" => "medium".yellow().to_string(),
            _ => "low".normal().to_string(),
        };

        let runtime_display = if entry.has_runtime_usage {
            "yes".green().to_string()
        } else {
            "no".normal().to_string()
        };

        let callsite_display = if entry.has_call_site {
            "yes".green().to_string()
        } else {
            "no".normal().to_string()
        };

        // Trim the timestamp to just the date+time part for display.
        let last_seen_short = entry
            .last_seen
            .get(..19)
            .unwrap_or(&entry.last_seen)
            .to_string();

        println!(
            "  {:<20}  {:<12}  {:<8}  {:<8}  {:<30}  {:<20}  {}",
            entry.consumer.name,
            confidence_display,
            runtime_display,
            callsite_display,
            last_seen_short,
            entry.consumer.owner_team,
            entry.consumer.contact,
        );
    }

    println!();
    println!(
        "  {} consumer(s) affected.",
        br.entries.len().to_string().bold()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_summary_serializes_all_fields() {
        let summary = CheckSummary {
            diff_id: Some("abc-123".to_string()),
            breaking_count: 2,
            affected_consumer_count: 1,
            policy_verdict: "block".to_string(),
            dashboard_url: Some("https://radar.example.com/app/diffs/abc-123".to_string()),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["diff_id"], "abc-123");
        assert_eq!(v["breaking_count"], 2);
        assert_eq!(v["affected_consumer_count"], 1);
        assert_eq!(v["policy_verdict"], "block");
        assert_eq!(v["dashboard_url"], "https://radar.example.com/app/diffs/abc-123");
    }

    #[test]
    fn check_summary_none_optionals_serialize_as_null() {
        let summary = CheckSummary {
            diff_id: None,
            breaking_count: 0,
            affected_consumer_count: 0,
            policy_verdict: "pass".to_string(),
            dashboard_url: None,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["diff_id"].is_null());
        assert_eq!(v["breaking_count"], 0);
        assert_eq!(v["policy_verdict"], "pass");
    }

    #[test]
    fn write_summary_creates_valid_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("summary.json");
        let summary = CheckSummary {
            diff_id: Some("x".to_string()),
            breaking_count: 1,
            affected_consumer_count: 0,
            policy_verdict: "warn".to_string(),
            dashboard_url: None,
        };
        write_summary(&path, &summary).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["breaking_count"], 1);
        assert_eq!(v["policy_verdict"], "warn");
    }
}
