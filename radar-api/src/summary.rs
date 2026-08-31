use crate::auth::CallerOrg;
use crate::errors::ApiError;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{Duration, Utc};
use serde_json::json;

// GET /v1/summary — KPI stats for the dashboard home page
pub(crate) async fn get_summary(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let cutoff_30 = (Utc::now() - Duration::days(30)).to_rfc3339();
    // Org isolation: scope every aggregate to the caller's org. The
    // `(? = '' OR ...)` guard makes the filter a no-op for the empty/desktop
    // org so single-tenant use still counts all rows. The org value is bound
    // twice because sqlx `Any` uses positional `?` placeholders.
    let org_id = caller.sql_scope().to_string();

    let breaking_row = q!(r#"
        SELECT COUNT(*) AS cnt FROM change c
        JOIN diff d ON d.id = c.diff_id
        JOIN spec_version sv ON sv.id = d.to_version
        JOIN service s ON s.id = sv.service_id
        WHERE c.severity = 'breaking' AND d.created_at >= ?
          AND (? = '' OR s.org_id = ?)
        "#,)
    .bind(&cutoff_30)
    .bind(&org_id)
    .bind(&org_id)
    .fetch_one(&pool)
    .await?;
    let breaking_changes_30d: i64 = breaking_row.try_get("cnt").unwrap_or(0);

    let consumers_row = q!(r#"
        SELECT COUNT(DISTINCT s.consumer_id) AS cnt FROM subscription s
        WHERE EXISTS (
            SELECT 1 FROM diff d
            JOIN spec_version sv ON sv.id = d.to_version
            JOIN service svc     ON svc.id = sv.service_id
            JOIN change c        ON c.diff_id = d.id
            WHERE sv.service_id  = s.service_id
              AND c.severity     = 'breaking'
              AND d.created_at  >= ?
              AND (? = '' OR svc.org_id = ?)
        )
        "#,)
    .bind(&cutoff_30)
    .bind(&org_id)
    .bind(&org_id)
    .fetch_one(&pool)
    .await?;
    let consumers_at_risk: i64 = consumers_row.try_get("cnt").unwrap_or(0);

    let services_row = q!("SELECT COUNT(*) AS cnt FROM service WHERE (? = '' OR org_id = ?)")
        .bind(&org_id)
        .bind(&org_id)
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
