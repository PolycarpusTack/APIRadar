use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockOn {
    Never,
    /// Block on any breaking change, regardless of who we know about.
    ///
    /// This is the default: on a fresh install nobody has instrumented
    /// anything, so `ActiveConsumers` would let every breaking change through
    /// while reporting success. A team earns the narrower policy once its
    /// evidence coverage is real.
    #[default]
    AnyBreak,
    ActiveConsumers,
}

/// What we actually know about consumers of the service being checked.
///
/// The previous `has_active_consumers: bool` collapsed two very different
/// situations into `false`: "we looked and nobody uses this" and "nobody has
/// told us anything yet". Only the first is a reason to let a breaking change
/// through; the second is the absence of an answer, and treating it as a pass
/// is what made a fresh install silently permissive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerEvidence {
    /// At least one consumer appears in the blast radius.
    Affected,
    /// The blast radius is empty AND this service has evidence on file, so the
    /// emptiness is a real answer.
    NoneAffected,
    /// The blast radius is empty and there is no evidence for this service at
    /// all — we do not know who calls it.
    Unknown,
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
    /// Blocked because there is no evidence for this service, so an empty
    /// blast radius cannot be trusted to mean "nobody is affected".
    InsufficientCoverage,
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
    let config: DriftConfig =
        serde_yml::from_str(&content).map_err(|e| anyhow::anyhow!("invalid .radar.yml: {e}"))?;
    Ok(config)
}

impl Verdict {
    /// The value persisted via `POST /v1/policy-decisions` and used in PR
    /// comments.
    ///
    /// `InsufficientCoverage` reports as `"block"` on the wire: the
    /// `policy_decision` table documents only pass|warn|block|overridden, and
    /// the dashboard switches on those four. It *is* a block, so this is
    /// accurate rather than merely convenient — but recording the distinction
    /// server-side (so teams can count how often checks are blocked for lack
    /// of instrumentation rather than for a real breaking change) needs a
    /// migration and a UI change, and is deliberately left as a follow-up.
    pub fn wire_str(&self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Warn => "warn",
            Verdict::Block | Verdict::InsufficientCoverage => "block",
            Verdict::Overridden => "overridden",
        }
    }

    /// Human-facing label, which does distinguish the two blocking reasons —
    /// one is fixed by changing the API, the other by instrumenting consumers.
    pub fn human_str(&self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Warn => "WARN",
            Verdict::Block => "BLOCKED",
            Verdict::Overridden => "OVERRIDDEN",
            Verdict::InsufficientCoverage => "BLOCKED (insufficient evidence coverage)",
        }
    }
}

/// Determine the process exit code given the changes and policy.
///
/// `has_label_override` should be `true` when `policy.allow_override_with` is configured and
/// the matching label was found on the PR — this forces exit code 0 regardless of breaks.
pub fn exit_code(
    changes: &[radar_core::diff::DiffChange],
    policy: &PolicyConfig,
    consumers: ConsumerEvidence,
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
            if !has_breaking {
                return 0;
            }
            match consumers {
                // Someone we know about is affected.
                ConsumerEvidence::Affected => 1,
                // We have evidence for this service and none of it points at
                // the changed surface — an informed pass.
                ConsumerEvidence::NoneAffected => 0,
                // No evidence at all. The blast radius being empty tells us
                // nothing, so treat it as unresolved rather than as safe.
                ConsumerEvidence::Unknown => 1,
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
    consumers: ConsumerEvidence,
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
            // When the API was unreachable we have no *consumer* data, but the
            // label override (from GitHub) and `block_on` (from local .radar.yml)
            // do not depend on the Radar API — so honor both.  Only the
            // `ActiveConsumers` branch, which needs consumer data we don't have,
            // falls back to the raw breaking-change signal.  When the API
            // responded successfully, apply the full policy so the exit code is
            // consistent with Closed mode — only the verdict stays at Warn.
            let code = if api_error {
                if has_label_override && policy.allow_override_with.is_some() {
                    0
                } else {
                    match &policy.block_on {
                        BlockOn::Never => 0,
                        // No consumer data available, so treat ActiveConsumers
                        // like AnyBreak: block on any breaking change.
                        BlockOn::AnyBreak | BlockOn::ActiveConsumers => {
                            if has_breaking {
                                1
                            } else {
                                0
                            }
                        }
                    }
                }
            } else {
                exit_code(changes, policy, consumers, has_label_override)
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
            let code = exit_code(changes, policy, consumers, has_label_override);
            let verdict = if code == 0 {
                Verdict::Pass
            } else if matches!(policy.block_on, BlockOn::ActiveConsumers)
                && consumers == ConsumerEvidence::Unknown
            {
                // Distinguish "we found an affected consumer" from "we have no
                // idea who the consumers are". Both block, but only the second
                // is fixed by instrumenting rather than by changing the API.
                Verdict::InsufficientCoverage
            } else {
                Verdict::Block
            };
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
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Warn,
            ConsumerEvidence::Affected,
            false,
            false,
        );
        assert_eq!(d.exit_code, 0, "warn mode must never block");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_api_error_uses_local_diff_breaking() {
        let p = PolicyConfig::default();
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            false,
            true,
        );
        assert_eq!(d.exit_code, 1, "local breaking diff in open mode → exit 1");
        assert_eq!(
            d.verdict,
            Verdict::Warn,
            "open mode → warn verdict even on exit 1"
        );
    }

    #[test]
    fn fail_mode_open_api_error_uses_local_diff_clean() {
        let p = PolicyConfig::default();
        let d = decide(
            &[],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            false,
            true,
        );
        assert_eq!(d.exit_code, 0, "clean diff in open mode → exit 0");
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_closed_api_error_blocks() {
        let p = PolicyConfig::default();
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Closed,
            ConsumerEvidence::NoneAffected,
            false,
            true,
        );
        assert_eq!(d.exit_code, 1, "closed mode + api error → blocked");
        assert_eq!(d.verdict, Verdict::Block);
    }

    // ---- FIT-01: coverage-aware ActiveConsumers ----

    /// The heart of the finding: on a fresh install nobody has instrumented
    /// anything, so the blast radius is empty — and the old default let the
    /// breaking change through while showing a green check.
    #[test]
    fn block_on_default_is_any_break() {
        assert!(
            matches!(BlockOn::default(), BlockOn::AnyBreak),
            "the default must not depend on evidence nobody has collected yet"
        );
    }

    #[test]
    fn active_consumers_blocks_when_coverage_is_unknown() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Closed,
            ConsumerEvidence::Unknown,
            false,
            false,
        );
        assert_eq!(d.exit_code, 1, "no evidence at all must not read as safe");
        assert_eq!(d.verdict, Verdict::InsufficientCoverage);
    }

    #[test]
    fn active_consumers_passes_when_evidence_exists_but_nobody_is_affected() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Closed,
            ConsumerEvidence::NoneAffected,
            false,
            false,
        );
        assert_eq!(
            d.exit_code, 0,
            "an informed 'nobody uses this' is a real pass"
        );
        assert_eq!(d.verdict, Verdict::Pass);
    }

    #[test]
    fn active_consumers_blocks_when_a_consumer_is_affected() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Closed,
            ConsumerEvidence::Affected,
            false,
            false,
        );
        assert_eq!(d.exit_code, 1);
        assert_eq!(d.verdict, Verdict::Block);
    }

    /// Unknown coverage is only a problem when something breaking is present.
    #[test]
    fn unknown_coverage_does_not_block_a_safe_diff() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(
            &[],
            &p,
            &FailMode::Closed,
            ConsumerEvidence::Unknown,
            false,
            false,
        );
        assert_eq!(d.exit_code, 0);
        assert_eq!(d.verdict, Verdict::Pass);
    }

    /// An explicit label override still wins — coverage does not create a new
    /// way to be stuck.
    #[test]
    fn label_override_still_beats_insufficient_coverage() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            allow_override_with: Some("radar-override".into()),
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Closed,
            ConsumerEvidence::Unknown,
            true,
            false,
        );
        assert_eq!(d.exit_code, 0);
        assert_eq!(d.verdict, Verdict::Overridden);
    }

    #[test]
    fn insufficient_coverage_is_a_block_on_the_wire() {
        // The dashboard and policy_decision table know only four values, so the
        // new verdict must not leak an unrecognised string into either.
        assert_eq!(Verdict::InsufficientCoverage.wire_str(), "block");
        assert_eq!(Verdict::Block.wire_str(), "block");
        assert_eq!(Verdict::Pass.wire_str(), "pass");
        assert_eq!(Verdict::Warn.wire_str(), "warn");
        assert_eq!(Verdict::Overridden.wire_str(), "overridden");
    }

    #[test]
    fn humans_can_tell_the_two_blocking_reasons_apart() {
        assert_ne!(
            Verdict::InsufficientCoverage.human_str(),
            Verdict::Block.human_str(),
            "one is fixed by changing the API, the other by instrumenting consumers"
        );
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
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::Affected, false),
            0
        );
    }

    #[test]
    fn any_break_blocks_on_breaking() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            ..Default::default()
        };
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::NoneAffected, false),
            1
        );
    }

    #[test]
    fn active_consumers_blocks_only_when_consumer_exists() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::NoneAffected, false),
            0
        );
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::Affected, false),
            1
        );
    }

    #[test]
    fn label_override_bypasses_any_block_policy() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            allow_override_with: Some("label:drift-ack".to_string()),
            ..Default::default()
        };
        // Without override label → blocks
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::NoneAffected, false),
            1
        );
        // With override label → passes
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::NoneAffected, true),
            0
        );
    }

    #[test]
    fn label_override_ignored_when_not_configured() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            allow_override_with: None,
            ..Default::default()
        };
        // has_label_override=true but no override configured → still blocks
        assert_eq!(
            exit_code(&[breaking()], &p, ConsumerEvidence::NoneAffected, true),
            1
        );
    }

    // ── A-4: FailMode::Open must apply block_on policy when API is reachable ──

    #[test]
    fn fail_mode_open_no_error_active_consumers_missing_does_not_block() {
        // block_on=ActiveConsumers, no active consumers → exit 0 even with breaking change
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            false,
            false,
        );
        assert_eq!(
            d.exit_code, 0,
            "no active consumers → should not block in open mode"
        );
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_no_error_active_consumers_present_exits_one() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::Affected,
            false,
            false,
        );
        assert_eq!(
            d.exit_code, 1,
            "active consumers + breaking → exit 1 in open mode"
        );
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_no_error_block_on_any_break_exits_one() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            false,
            false,
        );
        assert_eq!(
            d.exit_code, 1,
            "block_on=AnyBreak + breaking → exit 1 in open mode"
        );
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_no_error_block_on_never_exits_zero() {
        let p = PolicyConfig {
            block_on: BlockOn::Never,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::Affected,
            false,
            false,
        );
        assert_eq!(
            d.exit_code, 0,
            "block_on=Never → always exit 0 regardless of consumers"
        );
        assert_eq!(d.verdict, Verdict::Warn);
    }

    // ── M-12: fail-open + api_error must still honor override + block_on ──────
    // The label override comes from GitHub and `block_on` from local .radar.yml;
    // neither depends on the (down) Radar API, so both must be respected even
    // when consumer data is unavailable.

    #[test]
    fn fail_mode_open_api_error_label_override_passes() {
        // (a) Open + api_error + valid label override + breaking → pass (exit 0).
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            allow_override_with: Some("label:drift-ack".to_string()),
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            true,
            true,
        );
        assert_eq!(
            d.exit_code, 0,
            "valid label override honored in fail-open despite api error"
        );
        assert_eq!(d.verdict, Verdict::Warn, "open mode stays warn verdict");
    }

    #[test]
    fn fail_mode_open_api_error_block_on_never_passes() {
        // (b) Open + api_error + block_on=never + breaking → pass (exit 0).
        let p = PolicyConfig {
            block_on: BlockOn::Never,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            false,
            true,
        );
        assert_eq!(
            d.exit_code, 0,
            "block_on=never honored in fail-open despite api error"
        );
        assert_eq!(d.verdict, Verdict::Warn);
    }

    #[test]
    fn fail_mode_open_api_error_label_override_ignored_when_not_configured() {
        // has_label_override=true but allow_override_with=None → still block on breaking.
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            allow_override_with: None,
            ..Default::default()
        };
        let d = decide(
            &[breaking()],
            &p,
            &FailMode::Open,
            ConsumerEvidence::NoneAffected,
            true,
            true,
        );
        assert_eq!(
            d.exit_code, 1,
            "override not configured → breaking still blocks"
        );
    }
}
