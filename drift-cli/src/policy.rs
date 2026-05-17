use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockOn {
    Never,
    AnyBreak,
    #[default]
    ActiveConsumers,
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
}

impl DriftConfig {
    pub fn policy(&self) -> PolicyConfig {
        self.policy.clone().unwrap_or_default()
    }
}

/// Load .drift.yml from the given path (or default to ".drift.yml" in cwd).
/// Returns default config if the file doesn't exist.
pub fn load_config(path: Option<&std::path::Path>) -> anyhow::Result<DriftConfig> {
    let p = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".drift.yml"));
    if !p.exists() {
        return Ok(DriftConfig::default());
    }
    let content = std::fs::read_to_string(&p)?;
    let config: DriftConfig = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("invalid .drift.yml: {e}"))?;
    Ok(config)
}

/// Determine the process exit code given the changes and policy.
/// Returns 0 (clean), 1 (blocking), or keeps 0 when policy says never/active_consumers with no registered consumers.
pub fn exit_code(
    changes: &[drift_core::diff::DiffChange],
    policy: &PolicyConfig,
    has_active_consumers: bool,
) -> i32 {
    use drift_core::models::Severity;
    let has_breaking = changes.iter().any(|c| c.severity == Severity::Breaking);
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

#[cfg(test)]
mod tests {
    use super::*;
    use drift_core::{
        diff::DiffChange,
        models::{ChangeKind, Severity},
    };

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
        assert_eq!(exit_code(&[breaking()], &p, true), 0);
    }

    #[test]
    fn any_break_blocks_on_breaking() {
        let p = PolicyConfig {
            block_on: BlockOn::AnyBreak,
            ..Default::default()
        };
        assert_eq!(exit_code(&[breaking()], &p, false), 1);
    }

    #[test]
    fn active_consumers_blocks_only_when_consumer_exists() {
        let p = PolicyConfig {
            block_on: BlockOn::ActiveConsumers,
            ..Default::default()
        };
        assert_eq!(exit_code(&[breaking()], &p, false), 0); // no consumers yet
        assert_eq!(exit_code(&[breaking()], &p, true), 1);
    }
}
