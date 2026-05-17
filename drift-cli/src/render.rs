use colored::Colorize;
use drift_core::{
    diff::DiffChange,
    models::{ChangeKind, Severity},
};

/// Print the check results to stdout.
/// If `use_color` is false, suppress ANSI codes (also respects NO_COLOR env var).
pub fn print_table(changes: &[DiffChange], use_color: bool) {
    if !use_color {
        colored::control::set_override(false);
    }

    let title = "drift check — API Contract Drift Monitor".bold();
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
