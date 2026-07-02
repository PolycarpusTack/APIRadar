use crate::auth::JwtClaims;
use crate::errors::ApiError;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub(crate) struct CreatePolicyDecisionBody {
    pub(crate) diff_id: Option<String>,
    pub(crate) service_id: Option<String>,
    pub(crate) verdict: String,
    pub(crate) fail_mode: String,
    pub(crate) actor: Option<String>,
}

/// GET /v1/policy-decisions — list policy decisions for the org (paginated).
pub(crate) async fn list_policy_decisions(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .min(200);
    let offset: i64 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let rows = q!(
        "SELECT id, diff_id, service_id, verdict, fail_mode, actor, created_at \
         FROM policy_decision WHERE org_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(&org_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            use sqlx::Row;
            json!({
                "id":         r.get::<String, _>("id"),
                "diff_id":    r.get::<Option<String>, _>("diff_id"),
                "service_id": r.get::<Option<String>, _>("service_id"),
                "verdict":    r.get::<String, _>("verdict"),
                "fail_mode":  r.get::<String, _>("fail_mode"),
                "actor":      r.get::<Option<String>, _>("actor"),
                "created_at": r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(
        json!({ "entries": entries, "limit": limit, "offset": offset }),
    ))
}

// POST /v1/policy-decisions — persist a policy verdict from a drift check run
pub(crate) async fn create_policy_decision(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreatePolicyDecisionBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.verdict.is_empty() {
        return Err(ApiError::BadRequest("verdict must not be empty".into()));
    }
    if body.fail_mode.is_empty() {
        return Err(ApiError::BadRequest("fail_mode must not be empty".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    q!(
        "INSERT INTO policy_decision (id, org_id, diff_id, service_id, verdict, fail_mode, actor, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(&body.diff_id)
    .bind(&body.service_id)
    .bind(&body.verdict)
    .bind(&body.fail_mode)
    .bind(&body.actor)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":         id,
            "org_id":     org_id,
            "diff_id":    body.diff_id,
            "service_id": body.service_id,
            "verdict":    body.verdict,
            "fail_mode":  body.fail_mode,
            "actor":      body.actor,
            "created_at": now,
        })),
    ))
}
