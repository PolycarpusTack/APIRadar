use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::JwtClaims;
use crate::errors::ApiError;
use crate::sampling::load_sampling;
use crate::utils::{field_in_deny_list, normalise_path, otlp_attr, sample_keep};

#[derive(serde::Deserialize)]
pub(crate) struct UsageEventRequest {
    pub(crate) consumer_id: String,
    pub(crate) service_id: String,
    pub(crate) operation: String,
    #[serde(default)]
    pub(crate) field_path: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct CallSiteInput {
    pub(crate) consumer_id: String,
    pub(crate) service_id: String,
    #[serde(default)]
    pub(crate) operation: String,
    pub(crate) file_path: String,
    pub(crate) line_number: i64,
    pub(crate) field_path: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct GatewayLogEntry {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) consumer_id: String,
    pub(crate) service_id: String,
    #[serde(default)]
    #[allow(dead_code)] // retained for future status-code-based filtering
    pub(crate) status_code: Option<u16>,
}

fn call_site_id(
    consumer_id: &str,
    service_id: &str,
    file_path: &str,
    line_number: i64,
    field_path: &str,
) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{consumer_id}/{service_id}/{file_path}/{line_number}/{field_path}").as_bytes(),
    )
    .to_string()
}

// POST /v1/usage/events
pub(crate) async fn ingest_usage_event(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(events): Json<Vec<UsageEventRequest>>,
) -> Result<impl IntoResponse, ApiError> {
    if events.len() > 500 {
        return Err(ApiError::TooManyRequests(
            "batch too large, max 500".to_string(),
        ));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    // Phase 1 — filtering (reads only). Done before the transaction is opened so we
    // never hold a write connection while issuing sampling queries.
    let mut to_insert: Vec<&UsageEventRequest> = Vec::with_capacity(events.len());
    for event in &events {
        if !event.field_path.is_empty() {
            let sampling = load_sampling(&pool, &event.service_id, &org_id).await;
            let deny_str = sampling.field_deny_list.join(",");
            if field_in_deny_list(&event.field_path, &deny_str) { continue; }
        }
        to_insert.push(event);
    }

    // Phase 2 — atomic batch insert. A mid-batch failure rolls the whole batch back,
    // so a client retry never leaves partial duplicates. A FK/check violation from
    // bad input maps to 4xx (see map_ingest_db_error) rather than a 500.
    let mut tx = pool.begin().await?;
    for event in &to_insert {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(&event.consumer_id)
        .bind(&event.service_id)
        .bind(&event.operation)
        .bind(&event.field_path)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(crate::errors::map_ingest_db_error)?;
    }
    tx.commit().await?;

    Ok((StatusCode::ACCEPTED, Json(json!({"accepted": to_insert.len()}))))
}

// POST /v1/call-sites
pub(crate) async fn upsert_call_sites(
    State(pool): State<sqlx::AnyPool>,
    Json(sites): Json<Vec<CallSiteInput>>,
) -> Result<impl IntoResponse, ApiError> {
    if sites.len() > 5000 {
        return Err(ApiError::TooManyRequests(
            "batch too large, max 5000".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let count = sites.len();

    // Wrap the whole upsert batch in one transaction so a mid-batch failure rolls
    // back rather than leaving a partial commit. The deterministic call_site_id keeps
    // each row idempotent across retries; a FK/check violation maps to 4xx.
    let mut tx = pool.begin().await?;
    for site in &sites {
        let id = call_site_id(
            &site.consumer_id,
            &site.service_id,
            &site.file_path,
            site.line_number,
            &site.field_path,
        );

        let updated = sqlx::query(
            "UPDATE call_site SET last_seen_at = ?, operation = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(&site.operation)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(crate::errors::map_ingest_db_error)?;

        if updated.rows_affected() == 0 {
            sqlx::query(
                r#"INSERT INTO call_site
                   (id, consumer_id, service_id, operation, file_path, line_number, field_path, last_seen_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&id)
            .bind(&site.consumer_id)
            .bind(&site.service_id)
            .bind(&site.operation)
            .bind(&site.file_path)
            .bind(site.line_number)
            .bind(&site.field_path)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(crate::errors::map_ingest_db_error)?;
        }
    }
    tx.commit().await?;

    Ok((StatusCode::ACCEPTED, Json(json!({"accepted": count}))))
}

/// POST /v1/otlp/v1/traces — accept OTLP JSON trace export.
pub(crate) async fn ingest_otlp_traces(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    let mut accepted = 0usize;

    let resource_spans = body
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for rs in &resource_spans {
        let resource_attrs = rs
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();
        let resource_service_name = otlp_attr(&resource_attrs, "service.name");

        let scope_spans = rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for ss in &scope_spans {
            let spans = ss
                .get("spans")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for span in &spans {
                let kind = span.get("kind").and_then(|v| v.as_i64()).unwrap_or(0);
                if kind != 3 { continue; }

                let attrs = span
                    .get("attributes")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                let consumer_id = match otlp_attr(&attrs, "radar.consumer_id") {
                    Some(id) => id,
                    None => match &resource_service_name {
                        Some(name) => {
                            let found: Option<String> = sqlx::query_scalar(
                                "SELECT id FROM consumer WHERE org_id = ? AND name = ? LIMIT 1",
                            )
                            .bind(&org_id)
                            .bind(name)
                            .fetch_optional(&pool)
                            .await
                            .unwrap_or(None);
                            match found {
                                Some(id) => id,
                                None => continue,
                            }
                        }
                        None => continue,
                    },
                };

                let service_id = match otlp_attr(&attrs, "radar.service_id") {
                    Some(id) => id,
                    None => continue,
                };

                let method = otlp_attr(&attrs, "http.method").unwrap_or_default();
                let route = otlp_attr(&attrs, "http.route")
                    .or_else(|| otlp_attr(&attrs, "http.target").map(|p| normalise_path(&p)))
                    .unwrap_or_default();

                if method.is_empty() || route.is_empty() { continue; }
                let operation = format!("{} {}", method.to_uppercase(), route);

                let sampling = load_sampling(&pool, &service_id, &org_id).await;
                if !sample_keep(sampling.sample_rate) { continue; }

                let id = Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&consumer_id)
                .bind(&service_id)
                .bind(&operation)
                .bind("")
                .bind(&now)
                .execute(&pool)
                .await;

                accepted += 1;
            }
        }
    }

    Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": accepted }))))
}

/// POST /v1/gateway/logs
pub(crate) async fn ingest_gateway_logs(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(entries): Json<Vec<GatewayLogEntry>>,
) -> Result<impl IntoResponse, ApiError> {
    if entries.len() > 5000 {
        return Err(ApiError::TooManyRequests("batch too large, max 5000".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    let mut accepted = 0usize;

    for entry in &entries {
        let operation = format!(
            "{} {}",
            entry.method.to_uppercase(),
            normalise_path(&entry.path),
        );

        let sampling = load_sampling(&pool, &entry.service_id, &org_id).await;
        if !sample_keep(sampling.sample_rate) { continue; }

        let id = Uuid::new_v4().to_string();
        let _ = sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&entry.consumer_id)
        .bind(&entry.service_id)
        .bind(&operation)
        .bind("")
        .bind(&now)
        .execute(&pool)
        .await;

        accepted += 1;
    }

    Ok((StatusCode::ACCEPTED, Json(json!({ "accepted": accepted }))))
}
