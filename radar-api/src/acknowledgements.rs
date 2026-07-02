use crate::auth::{require_org_owned, JwtClaims, OrgResource};
use crate::errors::ApiError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub(crate) struct CreateAcknowledgementBody {
    pub(crate) diff_id: Option<String>,
    pub(crate) change_id: Option<String>,
    pub(crate) consumer_id: Option<String>,
    pub(crate) service_id: Option<String>,
    pub(crate) acknowledged_by: String,
    pub(crate) reason: Option<String>,
    pub(crate) expires_at: Option<String>,
}

/// POST /v1/acknowledgements — create a formal acknowledgement of a breaking change impact.
pub(crate) async fn create_acknowledgement(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateAcknowledgementBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.acknowledged_by.is_empty() {
        return Err(ApiError::BadRequest(
            "acknowledged_by must not be empty".into(),
        ));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    // Org isolation: the referenced resources must belong to the caller's org
    // before we record an acknowledgement (or post a GitHub status) for them.
    if let Some(ref diff_id) = body.diff_id {
        require_org_owned(&pool, OrgResource::Diff, diff_id, &org_id).await?;
    }
    if let Some(ref service_id) = body.service_id {
        require_org_owned(&pool, OrgResource::Service, service_id, &org_id).await?;
    }
    if let Some(ref consumer_id) = body.consumer_id {
        require_org_owned(&pool, OrgResource::Consumer, consumer_id, &org_id).await?;
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    q!(
        "INSERT INTO acknowledgement \
         (id, org_id, diff_id, change_id, consumer_id, service_id, acknowledged_by, reason, expires_at, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(&body.diff_id)
    .bind(&body.change_id)
    .bind(&body.consumer_id)
    .bind(&body.service_id)
    .bind(&body.acknowledged_by)
    .bind(&body.reason)
    .bind(&body.expires_at)
    .bind(&now)
    .execute(&pool)
    .await?;

    // K-6: post GitHub status check if this acknowledgement is for a diff with a PR URL
    if let Some(ref diff_id) = body.diff_id {
        use sqlx::Row;
        let pr_url: Option<String> = q!("SELECT pr_url FROM diff WHERE id = ?")
            .bind(diff_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.try_get("pr_url").ok());

        if let Some(Some(url)) = pr_url.map(Some) {
            tokio::spawn(async move {
                crate::notifications::post_github_status_acknowledged(&url).await;
            });
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":               id,
            "org_id":           org_id,
            "diff_id":          body.diff_id,
            "change_id":        body.change_id,
            "consumer_id":      body.consumer_id,
            "service_id":       body.service_id,
            "acknowledged_by":  body.acknowledged_by,
            "reason":           body.reason,
            "expires_at":       body.expires_at,
            "created_at":       now,
        })),
    ))
}

/// GET /v1/diffs/:id/acknowledgements — list active acknowledgements for a diff.
/// Excludes rows where expires_at is in the past.
pub(crate) async fn list_diff_acknowledgements(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Path(diff_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    let rows = q!(
        "SELECT id, diff_id, change_id, consumer_id, service_id, acknowledged_by, reason, expires_at, created_at \
         FROM acknowledgement \
         WHERE org_id = ? AND diff_id = ? \
           AND (expires_at IS NULL OR expires_at > ?) \
         ORDER BY created_at DESC",
    )
    .bind(&org_id)
    .bind(&diff_id)
    .bind(&now)
    .fetch_all(&pool)
    .await?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            use sqlx::Row;
            json!({
                "id":              r.get::<String, _>("id"),
                "diff_id":         r.get::<Option<String>, _>("diff_id"),
                "change_id":       r.get::<Option<String>, _>("change_id"),
                "consumer_id":     r.get::<Option<String>, _>("consumer_id"),
                "service_id":      r.get::<Option<String>, _>("service_id"),
                "acknowledged_by": r.get::<String, _>("acknowledged_by"),
                "reason":          r.get::<Option<String>, _>("reason"),
                "expires_at":      r.get::<Option<String>, _>("expires_at"),
                "created_at":      r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "diff_id": diff_id, "entries": entries })))
}

/// GET /v1/acknowledgements — list all acknowledgements for the org (paginated).
pub(crate) async fn list_acknowledgements(
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
        "SELECT id, diff_id, change_id, consumer_id, service_id, acknowledged_by, reason, expires_at, created_at \
         FROM acknowledgement \
         WHERE org_id = ? \
         ORDER BY created_at DESC \
         LIMIT ? OFFSET ?",
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
                "id":              r.get::<String, _>("id"),
                "diff_id":         r.get::<Option<String>, _>("diff_id"),
                "change_id":       r.get::<Option<String>, _>("change_id"),
                "consumer_id":     r.get::<Option<String>, _>("consumer_id"),
                "service_id":      r.get::<Option<String>, _>("service_id"),
                "acknowledged_by": r.get::<String, _>("acknowledged_by"),
                "reason":          r.get::<Option<String>, _>("reason"),
                "expires_at":      r.get::<Option<String>, _>("expires_at"),
                "created_at":      r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(
        json!({ "entries": entries, "limit": limit, "offset": offset }),
    ))
}
