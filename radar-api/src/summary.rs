use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde_json::json;
use crate::errors::ApiError;

// GET /v1/summary — KPI stats for the dashboard home page
pub(crate) async fn get_summary(
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let cutoff_30 = (Utc::now() - Duration::days(30)).to_rfc3339();

    let breaking_row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt FROM change c
        JOIN diff d ON d.id = c.diff_id
        WHERE c.severity = 'breaking' AND d.created_at >= ?
        "#,
    )
    .bind(&cutoff_30)
    .fetch_one(&pool)
    .await?;
    let breaking_changes_30d: i64 = breaking_row.try_get("cnt").unwrap_or(0);

    let consumers_row = sqlx::query(
        r#"
        SELECT COUNT(DISTINCT s.consumer_id) AS cnt FROM subscription s
        WHERE EXISTS (
            SELECT 1 FROM diff d
            JOIN spec_version sv ON sv.id = d.to_version
            JOIN change c        ON c.diff_id = d.id
            WHERE sv.service_id  = s.service_id
              AND c.severity     = 'breaking'
              AND d.created_at  >= ?
        )
        "#,
    )
    .bind(&cutoff_30)
    .fetch_one(&pool)
    .await?;
    let consumers_at_risk: i64 = consumers_row.try_get("cnt").unwrap_or(0);

    let services_row = sqlx::query("SELECT COUNT(*) AS cnt FROM service")
        .fetch_one(&pool)
        .await?;
    let services_count: i64 = services_row.try_get("cnt").unwrap_or(0);

    Ok((
        StatusCode::OK,
        Json(json!({
            "breaking_changes_30d": breaking_changes_30d,
            "consumers_at_risk":    consumers_at_risk,
            "services_count":       services_count,
        })),
    ))
}
