use crate::auth::CallerOrg;
use crate::errors::ApiError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

const VALID_CHANGE_KINDS: &[&str] = &[
    "field_removed",
    "field_added",
    "type_changed",
    "required_changed",
    "operation_removed",
    "operation_added",
    "parameter_removed",
    "response_removed",
    "enum_value_removed",
    "enum_value_added",
    "nullability_changed",
    "request_body_added",
    "request_body_removed",
];

#[derive(serde::Deserialize)]
pub(crate) struct CreateEvolutionRuleBody {
    pub(crate) name: String,
    pub(crate) change_kind: String,
    pub(crate) path_pattern: Option<String>,
    pub(crate) severity_override: String,
}

/// POST /v1/evolution-rules
pub(crate) async fn create_evolution_rule(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Json(body): Json<CreateEvolutionRuleBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.is_empty() {
        return Err(ApiError::Unprocessable("name is required".into()));
    }
    if !VALID_CHANGE_KINDS.contains(&body.change_kind.as_str()) {
        return Err(ApiError::Unprocessable(format!(
            "change_kind must be one of: {}",
            VALID_CHANGE_KINDS.join(", ")
        )));
    }
    if body.severity_override != "safe" && body.severity_override != "non_breaking_risky" {
        return Err(ApiError::Unprocessable(
            "severity_override must be 'safe' or 'non_breaking_risky'".into(),
        ));
    }

    let org_id = caller.sql_scope().to_string();
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    q!(
        "INSERT INTO evolution_rule (id, org_id, name, change_kind, path_pattern, severity_override, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(&body.name)
    .bind(&body.change_kind)
    .bind(body.path_pattern.as_deref())
    .bind(&body.severity_override)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":               id,
            "name":             body.name,
            "change_kind":      body.change_kind,
            "path_pattern":     body.path_pattern,
            "severity_override": body.severity_override,
            "enabled":          true,
        })),
    ))
}

/// GET /v1/evolution-rules
pub(crate) async fn list_evolution_rules(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let org_id = caller.sql_scope().to_string();

    let rows = q!(
        "SELECT id, name, change_kind, path_pattern, severity_override, enabled, created_at
         FROM evolution_rule
         WHERE org_id = ?
         ORDER BY created_at DESC",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":               r.get::<String, _>("id"),
                "name":             r.get::<String, _>("name"),
                "change_kind":      r.get::<String, _>("change_kind"),
                "path_pattern":     r.try_get::<Option<String>, _>("path_pattern").unwrap_or(None),
                "severity_override": r.get::<String, _>("severity_override"),
                "enabled":          r.get::<i64, _>("enabled") != 0,
                "created_at":       r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!({ "entries": entries }))))
}

/// DELETE /v1/evolution-rules/:id
pub(crate) async fn delete_evolution_rule(
    Path(rule_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();

    let result = q!("DELETE FROM evolution_rule WHERE id = ? AND org_id = ?")
        .bind(&rule_id)
        .bind(&org_id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!(
            "evolution rule {rule_id} not found"
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /v1/evolution-rules/:id — toggle enabled/disabled
pub(crate) async fn toggle_evolution_rule(
    Path(rule_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();
    let enabled: i64 = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .map(|b| if b { 1 } else { 0 })
        .ok_or_else(|| ApiError::Unprocessable("body must include enabled: bool".into()))?;

    let result = q!("UPDATE evolution_rule SET enabled = ? WHERE id = ? AND org_id = ?")
        .bind(enabled)
        .bind(&rule_id)
        .bind(&org_id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!(
            "evolution rule {rule_id} not found"
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}
