use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockOn {
    Never,
    AnyBreak,
    #[default]
    ActiveConsumers,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailMode {
    /// API errors are fatal — block the build. Default.
    #[default]
    Closed,
    /// API errors are tolerated — exit code from local diff only, verdict=warn.
    Open,
    /// Never fail the build — always exit 0, verdict=warn.
    Warn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Warn,
    Block,
    Overridden,
}

#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub verdict: Verdict,
    pub fail_mode: FailMode,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolicyConfig {
    pub block_on: BlockOn,
    #[serde(default = "default_lookback")]
    pub lookback_days: u32,
    pub allow_override_with: Option<String>,
}

fn default_lookback() -> u32 {
    30
}

impl Default for PolicyConfig {
    fn default() -> Self {
        PolicyConfig {
            block_on: BlockOn::default(),
            lookback_days: 30,
            allow_override_with: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DriftConfig {
    pub version: Option<u32>,
    pub service: Option<String>,
    pub policy: Option<PolicyConfig>,
    pub fail_mode: Option<FailMode>,
    /// Glob patterns for Postman/NativeREST collection files to scan automatically.
    /// e.g. ["**/*.postman_collection.json", "**/*.nativerest_collection.json"]
    #[serde(default)]
    pub collection_paths: Vec<String>,
}

impl DriftConfig {
    pub fn policy(&self) -> PolicyConfig {
        self.policy.clone().unwrap_or_default()
    }

    pub fn fail_mode(&self) -> FailMode {
        self.fail_mode.clone().unwrap_or_default()
    }
}

/// Load .radar.yml from the given path (or default to ".radar.yml" in cwd).
/// Returns default config if the file doesn't exist.
pub fn load_config(path: Option<&std::path::Path>) -> anyhow::Result<DriftConfig> {
    let p = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".radar.yml"));
    if !p.exists() {
        return Ok(DriftConfig::default());
    }
    let content = std::fs::read_to_string(&p)?;
    let config: DriftConfig = serde_yml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid .radar.yml: {e}"))?;
    Ok(config)
}

/// Determine the process exit code given the changes and policy.
///
/// `has_label_override` should be `true` when `policy.allow_override_with` is configured and
/// the matching label was found on the PR — this forces exit code 0 regardless of breaks.
pub fn exit_code(
    changes: &[radar_core::diff::DiffChange],
    policy: &PolicyConfig,
    has_active_consumers: bool,
    has_label_override: bool,
) -> i32 {
    use radar_core::models::Severity;
    let has_breaking = changes.iter().any(|c| c.severity == Severity::Breaking);

    // D-2: configured label on the PR overrides any blocking policy.
    if has_label_override && policy.allow_override_with.is_some() {
        return 0;
    }

    match &policy.block_on {
        BlockOn::Never => 0,
        BlockOn::AnyBreak => {
            if has_breaking {
                1
            } else {
                0
            }
        }
        BlockOn::ActiveConsumers => {
            if has_breaking && has_active_consumers {
                1
            } else {
                0
            }
        }
    }
}

/// Compute the full policy decision including fail_mode semantics.
///
/// `api_error` is `true` when the Radar API was unreachable or returned a server error.
pub fn decide(
    changes: &[radar_core::diff::DiffChange],
    policy: &PolicyConfig,
    fail_mode: &FailMode,
    has_active_consumers: bool,
    has_label_override: bool,
    api_error: bool,
) -> PolicyDecision {
    use radar_core::models::Severity;
    let has_breaking = changes.iter().any(|c| c.severity == Severity::Breaking);

    match fail_mode {
        FailMode::Warn => PolicyDecision {
            verdict: Verdict::Warn,
            fail_mode: FailMode::Warn,
            exit_code: 0,
        },
        FailMode::Open => {
            // When the API was unreachable we have no consumer data, so fall
            // back to the raw breaking-change signal.  When the API responded
            // successfully, apply the full policy (block_on + label override)
            // so that the exit code is consistent with what Closed mode would
            // produce — the only difference is the verdict stays at Warn.
            let code = if api_error {
                if has_breaking { 1 } else { 0 }
            } else {
                exit_code(changes, policy, has_active_consumers, has_label_override)
            };
            PolicyDecision {
                verdict: Verdict::Warn,
                fail_mode: FailMode::Open,
                exit_code: code,
            }
        }
        FailMode::Closed => {
            if api_error {
                return PolicyDecision {
                    verdict: Verdict::Block,
                    fail_mode: FailMode::Closed,
                    exit_code: 1,
                };
            }
            if has_label_override && policy.allow_override_with.is_some() {
                return PolicyDecision {
                    verdict: Verdict::Overridden,
                    fail_mode: FailMode::Closed,
                    exit_code: 0,
                };
            }
            let code = exit_code(changes, policy, has_active_consumers, has_label_override);
            let verdict = if code == 0 { Verdict::Pass } else { Verdict::Block };
            PolicyDecision {
                verdict,
                fail_mode: FailMode::Closed,
                exit_code: code,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_core::{
        diff::DiffChange,
        models::{ChangeKind, Severity},
    };

    // ── E-3-T1: fail_mode tests ──────────────────────────────────────────────

    #[test]
    fn fail_mode_warn_always_exits_zero_with_warn_verdict() {
        let p = PolicyConfig::default();
        let d = decide(&[breaking()], &p, &FailMode::Warn, true, false, false);
        assert_eq!(d.exit_code, 0, "warn mode must never block");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_api_error_uses_local_diff_breaking() {
        let p = PolicyConfig::default();
        let d = decide(&[breaking()], &p, &FailMode::Open, false, false, true);
        assert_eq!(d.exit_code, 1, "local breaking diff in open mode → exit 1");
        assert_eq!(d.verdict, Verdict::Warn, "open mode → warn verdict even on exit 1");
    }

    #[test]
    fn fail_mode_open_api_error_uses_local_diff_clean() {
        let p = PolicyConfig::default();
        let d = decide(&[], &p, &FailMode::Open, false, false, true);
        assert_eq!(d.exit_code, 0, "clean diff in open mode → exit 0");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_closed_api_error_blocks() {
        let p = PolicyConfig::default();
        let d = decide(&[breaking()], &p, &FailMode::Closed, false, false, true);
        assert_eq!(d.exit_code, 1, "closed mode + api error → blocked");
        assert_eq!(d.verdict, Verdict::Block);
    }

    #[test]
    fn fail_mode_default_is_closed() {
        assert_eq!(DriftConfig::default().fail_mode(), FailMode::Closed);
    }

    fn breaking() -> DiffChange {
        DiffChange {
            path: "GET /users".into(),
            kind: ChangeKind::OperationRemoved,
            severity: Severity::Breaking,
            description: None,
        }
    }

    #[test]
    fn never_always_zero() {
        let p = PolicyConfig {
            block_on: BlockOn::Never,
            lookback_days: 30,
            allow_override_with: None,
        };
        assert_eq!(exit_code(&[breaking()], &p, true, false), 0);
    }

    #[test]
    fn any_break_blocks_on_breaking() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            ..Default::default()
        };
        assert_eq!(exit_code(&[breaking()], &p, false, false), 1);
    }

    #[test]
    fn active_consumers_blocks_only_when_consumer_exists() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        assert_eq!(exit_code(&[breaking()], &p, false, false), 0);
        assert_eq!(exit_code(&[breaking()], &p, true, false), 1);
    }

    #[test]
    fn label_override_bypasses_any_block_policy() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            allow_override_with: Some("label:drift-ack".to_string()),
            ..Default::default()
        };
        // Without override label → blocks
        assert_eq!(exit_code(&[breaking()], &p, false, false), 1);
        // With override label → passes
        assert_eq!(exit_code(&[breaking()], &p, false, true), 0);
    }

    #[test]
    fn label_override_ignored_when_not_configured() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            allow_override_with: None,
            ..Default::default()
        };
        // has_label_override=true but no override configured → still blocks
        assert_eq!(exit_code(&[breaking()], &p, false, true), 1);
    }

    // ── A-4: FailMode::Open must apply block_on policy when API is reachable ──

    #[test]
    fn fail_mode_open_no_error_active_consumers_missing_does_not_block() {
        // block_on=ActiveConsumers, no active consumers → exit 0 even with breaking change
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(&[breaking()], &p, &FailMode::Open, false, false, false);
        assert_eq!(d.exit_code, 0, "no active consumers → should not block in open mode");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_no_error_active_consumers_present_exits_one() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(&[breaking()], &p, &FailMode::Open, true, false, false);
        assert_eq!(d.exit_code, 1, "active consumers + breaking → exit 1 in open mode");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_no_error_block_on_any_break_exits_one() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            ..Default::default()
        };
        let d = decide(&[breaking()], &p, &FailMode::Open, false, false, false);
        assert_eq!(d.exit_code, 1, "block_on=AnyBreak + breaking → exit 1 in open mode");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_no_error_block_on_never_exits_zero() {
        let p = PolicyConfig {
            block_on: BlockOn::Never,
            ..Default::default()
        };
        let d = decide(&[breaking()], &p, &FailMode::Open, true, false, false);
        assert_eq!(d.exit_code, 0, "block_on=Never → always exit 0 regardless of consumers");
        assert_eq!(d.verdict, Verdict::Warn);
    }
}
