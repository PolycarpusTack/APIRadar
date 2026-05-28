use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use crate::auth::JwtClaims;
use crate::errors::ApiError;

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct ServiceSamplingBody {
    #[serde(default = "default_sample_rate")]
    pub(crate) sample_rate: f64,
    #[serde(default)]
    pub(crate) field_deny_list: Vec<String>,
}

fn default_sample_rate() -> f64 { 1.0 }

/// Load sampling configuration for a service, returning defaults if not set.
pub(crate) async fn load_sampling(pool: &sqlx::AnyPool, service_id: &str, org_id: &str) -> ServiceSamplingBody {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT sample_rate, field_deny_list FROM service_sampling WHERE service_id = ? AND org_id = ?",
    )
    .bind(service_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some(r) => {
            let rate: f64 = r.try_get("sample_rate").unwrap_or(1.0);
            let deny_raw: String = r.try_get("field_deny_list").unwrap_or_default();
            let deny: Vec<String> = deny_raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            ServiceSamplingBody { sample_rate: rate, field_deny_list: deny }
        }
        None => ServiceSamplingBody { sample_rate: 1.0, field_deny_list: vec![] },
    }
}

/// GET /v1/services/:id/sampling
pub(crate) async fn get_sampling(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let config = load_sampling(&pool, &service_id, &org_id).await;
    Ok((StatusCode::OK, Json(json!({
        "service_id":       service_id,
        "sample_rate":      config.sample_rate,
        "field_deny_list":  config.field_deny_list,
    }))))
}

/// PUT /v1/services/:id/sampling
pub(crate) async fn put_sampling(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<ServiceSamplingBody>,
) -> Result<impl IntoResponse, ApiError> {
    if !(0.0..=1.0).contains(&body.sample_rate) {
        return Err(ApiError::BadRequest("sample_rate must be between 0.0 and 1.0".into()));
    }
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let deny_str = body.field_deny_list.join(",");
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO service_sampling (service_id, org_id, sample_rate, field_deny_list, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(service_id, org_id) DO UPDATE SET
             sample_rate = excluded.sample_rate,
             field_deny_list = excluded.field_deny_list,
             updated_at = excluded.updated_at",
    )
    .bind(&service_id)
    .bind(&org_id)
    .bind(body.sample_rate)
    .bind(&deny_str)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok((StatusCode::OK, Json(json!({
        "service_id":      service_id,
        "sample_rate":     body.sample_rate,
        "field_deny_list": body.field_deny_list,
    }))))
}

/// GET /v1/evidence/coverage — aggregated evidence stats by consumer × service × source type.
///
/// Returns a flat JSON array of `CoverageRow` objects so the UI can call `.filter()` directly.
/// Field names match the TypeScript `CoverageRow` interface in `EvidenceCoveragePage.tsx`.
pub(crate) async fn evidence_coverage(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let service_id = params.get("service_id").cloned().unwrap_or_default();

    // "Stale" = no recent evidence in the past 7 days.
    let stale_cutoff = (Utc::now() - Duration::days(7)).to_rfc3339();
    let now_str = Utc::now().to_rfc3339();

    let rows: Vec<_> = if service_id.is_empty() {
        sqlx::query(
            r#"SELECT ie.consumer_id,
                      COALESCE(c.name, ie.consumer_id) AS consumer_name,
                      ie.producer_service_id            AS service_id,
                      COALESCE(s.name, ie.producer_service_id) AS service_name,
                      ie.source_type,
                      COUNT(*)                          AS event_count,
                      MAX(ie.observed_at)               AS last_seen_at,
                      SUM(CASE WHEN ie.observed_at >= ? THEN 1 ELSE 0 END) AS recent
               FROM impact_evidence ie
               LEFT JOIN consumer c ON c.id = ie.consumer_id
               LEFT JOIN service  s ON s.id = ie.producer_service_id
               WHERE ie.org_id = ? AND (ie.expires_at IS NULL OR ie.expires_at > ?)
               GROUP BY ie.consumer_id, c.name, ie.producer_service_id, s.name, ie.source_type
               ORDER BY MAX(ie.observed_at) DESC"#,
        )
        .bind(&stale_cutoff)
        .bind(&org_id)
        .bind(&now_str)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT ie.consumer_id,
                      COALESCE(c.name, ie.consumer_id) AS consumer_name,
                      ie.producer_service_id            AS service_id,
                      COALESCE(s.name, ie.producer_service_id) AS service_name,
                      ie.source_type,
                      COUNT(*)                          AS event_count,
                      MAX(ie.observed_at)               AS last_seen_at,
                      SUM(CASE WHEN ie.observed_at >= ? THEN 1 ELSE 0 END) AS recent
               FROM impact_evidence ie
               LEFT JOIN consumer c ON c.id = ie.consumer_id
               LEFT JOIN service  s ON s.id = ie.producer_service_id
               WHERE ie.org_id = ? AND ie.producer_service_id = ?
                 AND (ie.expires_at IS NULL OR ie.expires_at > ?)
               GROUP BY ie.consumer_id, c.name, ie.producer_service_id, s.name, ie.source_type
               ORDER BY MAX(ie.observed_at) DESC"#,
        )
        .bind(&stale_cutoff)
        .bind(&org_id)
        .bind(&service_id)
        .bind(&now_str)
        .fetch_all(&pool)
        .await?
    };

    // Return a flat array — matches CoverageRow[] expected by the UI.
    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            let last_seen_at: String = r.try_get("last_seen_at").unwrap_or_default();
            let recent: i64 = r.try_get("recent").unwrap_or(0);
            let last_seen_at_opt: Option<String> = if last_seen_at.is_empty() { None } else { Some(last_seen_at) };
            json!({
                "consumer_id":   r.try_get::<String, _>("consumer_id").unwrap_or_default(),
                "consumer_name": r.try_get::<String, _>("consumer_name").unwrap_or_default(),
                "service_id":    r.try_get::<String, _>("service_id").unwrap_or_default(),
                "service_name":  r.try_get::<String, _>("service_name").unwrap_or_default(),
                "source_type":   r.try_get::<String, _>("source_type").unwrap_or_default(),
                "event_count":   r.try_get::<i64, _>("event_count").unwrap_or(0),
                "last_seen_at":  last_seen_at_opt,
                "is_stale":      recent == 0,
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(entries)))
}
