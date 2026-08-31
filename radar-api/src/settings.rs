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
pub(crate) async fn get_settings(
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    let rows = q!("SELECT key, value FROM settings")
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
        q!("INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",)
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

pub async fn purge_old_usage_events(
    pool: &sqlx::AnyPool,
    lookback_days: u32,
) -> anyhow::Result<u64> {
    let cutoff = (Utc::now() - Duration::days(lookback_days as i64)).to_rfc3339();
    let result = q!("DELETE FROM usage_event WHERE recorded_at < ?")
        .bind(&cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
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
