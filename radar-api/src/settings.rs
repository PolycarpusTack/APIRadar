use crate::auth::CallerOrg;
use crate::errors::ApiError;
use axum::{extract::State, response::IntoResponse, Json};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct AppSettings {
    policy_block_on: String,
    policy_lookback_days: i64,
    policy_allow_override_with: Option<String>,
    retention_days: i64,
}

// GET /v1/settings
//
// N-29: settings are per-org. Before this, one tenant's PUT rewrote the whole
// installation's policy and retention window.
pub(crate) async fn get_settings(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    let rows = q!("SELECT key, value FROM settings WHERE org_id = ?")
        .bind(caller.sql_scope())
        .fetch_all(&pool)
        .await?;

    let mut map: HashMap<String, String> = rows
        .iter()
        .map(|r| {
            use sqlx::Row;
            (r.get::<String, _>("key"), r.get::<String, _>("value"))
        })
        .collect();

    Ok(Json(AppSettings {
        policy_block_on: map
            .remove("policy.block_on")
            .unwrap_or_else(|| "active_consumers".to_string()),
        policy_lookback_days: map
            .remove("policy.lookback_days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
        policy_allow_override_with: map
            .remove("policy.allow_override_with")
            .filter(|s| !s.is_empty()),
        retention_days: map
            .remove("retention.days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(90),
    }))
}

// PUT /v1/settings
pub(crate) async fn update_settings(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Json(body): Json<AppSettings>,
) -> Result<impl IntoResponse, ApiError> {
    if !["never", "any_break", "active_consumers"].contains(&body.policy_block_on.as_str()) {
        return Err(ApiError::Unprocessable(
            "policy_block_on must be one of: never, any_break, active_consumers".into(),
        ));
    }
    if !(1..=365).contains(&body.policy_lookback_days) {
        return Err(ApiError::Unprocessable(
            "policy_lookback_days must be between 1 and 365".into(),
        ));
    }
    if !(1..=3650).contains(&body.retention_days) {
        return Err(ApiError::Unprocessable(
            "retention_days must be between 1 and 3650".into(),
        ));
    }

    let pairs = [
        ("policy.block_on", body.policy_block_on.clone()),
        (
            "policy.lookback_days",
            body.policy_lookback_days.to_string(),
        ),
        (
            "policy.allow_override_with",
            body.policy_allow_override_with.clone().unwrap_or_default(),
        ),
        ("retention.days", body.retention_days.to_string()),
    ];

    for (key, value) in &pairs {
        q!("INSERT INTO settings (org_id, key, value) VALUES (?, ?, ?)
             ON CONFLICT(org_id, key) DO UPDATE SET value = excluded.value",)
        .bind(caller.sql_scope())
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await?;
    }

    Ok(Json(body))
}

// GET /v1/settings/integrations — checks env vars server-side, returns booleans only
pub(crate) async fn get_integrations() -> Json<Value> {
    let configured = |key: &str| std::env::var(key).map(|v| !v.is_empty()).unwrap_or(false);
    let openai_key = configured("OPENAI_API_KEY");
    Json(json!({
        "anthropic":         configured("ANTHROPIC_API_KEY"),
        "openai":            openai_key,
        "openai_enterprise": openai_key && configured("OPENAI_BASE_URL"),
        "github_copilot":    configured("GITHUB_COPILOT_TOKEN"),
        "jira":              configured("JIRA_BASE_URL") && configured("JIRA_EMAIL") && configured("JIRA_TOKEN"),
        "github":            configured("GITHUB_TOKEN"),
        "postman":           configured("POSTMAN_API_KEY"),
    }))
}

/// Every org that has configured its own `retention.days`, with that value.
///
/// N-29: retention is a per-org setting, so the job can no longer apply one
/// tenant's window to the whole installation.
pub async fn configured_retention_days(pool: &sqlx::AnyPool) -> anyhow::Result<Vec<(String, u32)>> {
    use sqlx::Row;
    let rows = q!("SELECT org_id, value FROM settings WHERE key = 'retention.days'")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let org: String = r.get("org_id");
            let raw: String = r.get("value");
            raw.parse::<u32>().ok().map(|days| (org, days))
        })
        .collect())
}

/// Purge usage events older than `lookback_days`.
///
/// `scope` selects which rows are eligible:
/// * `RetentionScope::Org(id)` — only rows whose Producer belongs to that org.
/// * `RetentionScope::UnconfiguredOrgs(ids)` — every org EXCEPT those, i.e. the
///   installations running on the default window.
pub async fn purge_old_usage_events_scoped(
    pool: &sqlx::AnyPool,
    lookback_days: u32,
    scope: &RetentionScope<'_>,
) -> anyhow::Result<u64> {
    let cutoff = (Utc::now() - Duration::days(lookback_days as i64)).to_rfc3339();
    let sql = format!(
        "DELETE FROM usage_event WHERE recorded_at < ? AND service_id IN          (SELECT id FROM service WHERE {})",
        scope.org_predicate()
    );
    let sql = crate::db::pg(&sql);
    let mut query = sqlx::query(&sql).bind(&cutoff);
    for org in scope.bind_values() {
        query = query.bind(org.to_string());
    }
    Ok(query.execute(pool).await?.rows_affected())
}

/// Purge finished CSV run jobs older than `lookback_days`, within `scope`.
pub async fn purge_old_csv_runs_scoped(
    pool: &sqlx::AnyPool,
    lookback_days: u32,
    scope: &RetentionScope<'_>,
) -> anyhow::Result<u64> {
    let cutoff = (Utc::now() - Duration::days(lookback_days as i64)).to_rfc3339();
    let sql = format!(
        "DELETE FROM csv_run_job WHERE status IN          ('completed', 'completed_with_failures', 'failed', 'cancelled')          AND created_at < ? AND {}",
        scope.org_predicate()
    );
    let sql = crate::db::pg(&sql);
    let mut query = sqlx::query(&sql).bind(&cutoff);
    for org in scope.bind_values() {
        query = query.bind(org.to_string());
    }
    Ok(query.execute(pool).await?.rows_affected())
}

/// Which orgs a retention pass applies to.
pub enum RetentionScope<'a> {
    /// Exactly this org.
    Org(&'a str),
    /// Every org except these (the ones running on the default window).
    UnconfiguredOrgs(&'a [String]),
}

impl RetentionScope<'_> {
    /// The `WHERE`-clause fragment constraining `org_id`, with `?`
    /// placeholders — routed through `pg()` by `q!`, so it is portable.
    fn org_predicate(&self) -> String {
        match self {
            RetentionScope::Org(_) => "org_id = ?".to_string(),
            RetentionScope::UnconfiguredOrgs([]) => "1 = 1".to_string(),
            RetentionScope::UnconfiguredOrgs(orgs) => {
                let marks = vec!["?"; orgs.len()].join(", ");
                format!("org_id NOT IN ({marks})")
            }
        }
    }

    fn bind_values(&self) -> Vec<&str> {
        match self {
            RetentionScope::Org(id) => vec![id],
            RetentionScope::UnconfiguredOrgs(orgs) => orgs.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Delete impact_evidence rows whose expires_at has passed.
/// Rows with expires_at = NULL are never deleted by this job.
pub async fn expire_old_evidence(pool: &sqlx::AnyPool) -> anyhow::Result<u64> {
    let now = Utc::now().to_rfc3339();
    let result = q!("DELETE FROM impact_evidence WHERE expires_at IS NOT NULL AND expires_at < ?")
        .bind(&now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
