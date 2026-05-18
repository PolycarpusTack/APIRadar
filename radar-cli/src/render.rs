use colored::Colorize;
use radar_core::{
    diff::DiffChange,
    models::{ChangeKind, Severity},
};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Blast-radius response types (deserialized from the API)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ConsumerInfo {
    pub name: String,
    pub owner_team: String,
    pub contact: String,
}

#[derive(Deserialize)]
pub struct BlastRadiusEntry {
    pub consumer: ConsumerInfo,
    pub confidence: String,
    pub last_seen: String,
    pub has_runtime_usage: bool,
    pub has_call_site: bool,
}

#[derive(Deserialize)]
pub struct BlastRadiusResponse {
    pub entries: Vec<BlastRadiusEntry>,
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
        let kind_str = kind_label(&change.kind);
        println!("{badge}   {path_colored:<45}  {kind_str}");
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

fn kind_label(kind: &ChangeKind) -> &'static str {
    match kind {
        ChangeKind::FieldRemoved => "field_removed",
        ChangeKind::FieldAdded => "field_added",
        ChangeKind::TypeChanged => "type_changed",
        ChangeKind::RequiredChanged => "required_changed",
        ChangeKind::OperationRemoved => "operation_removed",
        ChangeKind::OperationAdded => "operation_added",
    }
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
