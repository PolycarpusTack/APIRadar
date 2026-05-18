use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the compiled `radar` binary.
fn drift_bin() -> std::path::PathBuf {
    // CARGO_BIN_EXE_radar is set automatically by cargo for [[bin]] targets.
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_radar"))
}

/// Write YAML content to a temp file and return it (kept alive by the caller).
fn temp_yaml(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("failed to create temp file");
    f.write_all(content.as_bytes())
        .expect("failed to write temp file");
    f
}

// ---------------------------------------------------------------------------
// Spec fixtures
// ---------------------------------------------------------------------------

const BASE_WITH_PHONE: &str = r#"
openapi: "3.0.0"
info:
  title: Users API
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
                  phone:
                    type: string
"#;

const HEAD_WITHOUT_PHONE: &str = r#"
openapi: "3.0.0"
info:
  title: Users API
  version: "1"
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
"#;

const MINIMAL_SPEC: &str = r#"
openapi: "3.0.0"
info:
  title: Test
  version: "1"
paths:
  /health:
    get:
      responses:
        '200':
          description: ok
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A breaking field removal is detected and printed, but the default policy
/// (ActiveConsumers) exits 0 when there are no registered consumers (P0).
#[test]
fn check_detects_breaking_field_removal() {
    let base_file = temp_yaml(BASE_WITH_PHONE);
    let head_file = temp_yaml(HEAD_WITHOUT_PHONE);

    let output = Command::new(drift_bin())
        .args([
            "check",
            "--base",
            base_file.path().to_str().unwrap(),
            "--head",
            head_file.path().to_str().unwrap(),
            "--no-color",
        ])
        .output()
        .expect("failed to execute drift binary");

    // Default policy = ActiveConsumers, no consumers → exit 0
    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0 (no active consumers), got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BREAKING"),
        "expected 'BREAKING' in stdout, got:\n{stdout}"
    );
    assert!(
        stdout.contains("phone"),
        "expected 'phone' in stdout, got:\n{stdout}"
    );
}

/// With --policy pointing to a file that sets block_on: any_break, a breaking
/// change causes exit code 1.
#[test]
fn check_any_break_policy_exits_one_on_breaking() {
    let base_file = temp_yaml(BASE_WITH_PHONE);
    let head_file = temp_yaml(HEAD_WITHOUT_PHONE);

    let policy_content = "policy:\n  block_on: any_break\n";
    let policy_file = temp_yaml(policy_content);

    let output = Command::new(drift_bin())
        .args([
            "check",
            "--base",
            base_file.path().to_str().unwrap(),
            "--head",
            head_file.path().to_str().unwrap(),
            "--no-color",
            "--policy",
            policy_file.path().to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute drift binary");

    assert_eq!(
        output.status.code(),
        Some(1),
        "expected exit 1 (any_break policy), got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// JSON output is a valid JSON array and contains at least one entry for the
/// breaking field removal.
#[test]
fn check_json_output_is_valid() {
    let base_file = temp_yaml(BASE_WITH_PHONE);
    let head_file = temp_yaml(HEAD_WITHOUT_PHONE);

    let output = Command::new(drift_bin())
        .args([
            "check",
            "--base",
            base_file.path().to_str().unwrap(),
            "--head",
            head_file.path().to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("failed to execute drift binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is not valid JSON");

    let arr = parsed.as_array().expect("expected a JSON array");
    assert!(
        !arr.is_empty(),
        "expected at least one change in JSON output"
    );

    // Check at least one entry mentions "phone"
    let mentions_phone = arr.iter().any(|v| {
        v.get("path")
            .and_then(|p| p.as_str())
            .map(|p| p.contains("phone"))
            .unwrap_or(false)
    });
    assert!(mentions_phone, "no JSON entry mentions 'phone': {stdout}");
}

/// Identical specs produce no changes and exit 0.
#[test]
fn check_identical_specs_exit_zero() {
    let base_file = temp_yaml(MINIMAL_SPEC);
    let head_file = temp_yaml(MINIMAL_SPEC);

    let output = Command::new(drift_bin())
        .args([
            "check",
            "--base",
            base_file.path().to_str().unwrap(),
            "--head",
            head_file.path().to_str().unwrap(),
            "--no-color",
        ])
        .output()
        .expect("failed to execute drift binary");

    assert_eq!(
        output.status.code(),
        Some(0),
        "expected exit 0 for identical specs, got: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No changes detected"),
        "expected 'No changes detected' in stdout, got:\n{stdout}"
    );
}
