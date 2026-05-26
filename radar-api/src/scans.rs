use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::JwtClaims;
use crate::errors::ApiError;
use crate::utils::is_ssrf_blocked;
use crate::webhooks::dispatch_diff_event;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct CreateScanBody {
    service_id: String,
    spec_url: String,
    #[serde(default = "default_format")]
    format: String,
    #[serde(default = "default_interval")]
    interval_minutes: i32,
}

fn default_format() -> String { "openapi".to_string() }
fn default_interval() -> i32 { 60 }

#[derive(Serialize)]
struct ScanResponse {
    id: String,
    org_id: String,
    service_id: String,
    spec_url: String,
    format: String,
    interval_minutes: i32,
    last_run_at: Option<String>,
    active: bool,
    created_at: String,
}

fn row_to_response(row: &sqlx::any::AnyRow) -> ScanResponse {
    ScanResponse {
        id: row.get("id"),
        org_id: row.get("org_id"),
        service_id: row.get("service_id"),
        spec_url: row.get("spec_url"),
        format: row.get("format"),
        interval_minutes: row.get("interval_minutes"),
        last_run_at: row.try_get("last_run_at").ok(),
        active: { let v: i32 = row.get("active"); v != 0 },
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// POST /v1/scheduled-scans
// ---------------------------------------------------------------------------

pub(crate) async fn create_scan(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateScanBody>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    if body.interval_minutes < 15 {
        return Err(ApiError::BadRequest("interval_minutes must be at least 15".into()));
    }
    if body.service_id.is_empty() {
        return Err(ApiError::BadRequest("service_id is required".into()));
    }
    if body.spec_url.is_empty() {
        return Err(ApiError::BadRequest("spec_url is required".into()));
    }
    if is_ssrf_blocked(&body.spec_url) {
        return Err(ApiError::BadRequest(
            "spec_url must be a reachable HTTPS endpoint outside private address space".into(),
        ));
    }

    // Upsert on (org_id, service_id, spec_url)
    let existing = sqlx::query(
        "SELECT id FROM scheduled_scan WHERE org_id = ? AND service_id = ? AND spec_url = ?",
    )
    .bind(&org_id)
    .bind(&body.service_id)
    .bind(&body.spec_url)
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        let id: String = row.get("id");
        sqlx::query(
            "UPDATE scheduled_scan SET interval_minutes = ?, active = 1 WHERE id = ?",
        )
        .bind(body.interval_minutes)
        .bind(&id)
        .execute(&pool)
        .await?;
        let updated = sqlx::query(
            "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, active, created_at FROM scheduled_scan WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await?;
        return Ok((StatusCode::OK, Json(row_to_response(&updated))));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO scheduled_scan (id, org_id, service_id, spec_url, format, interval_minutes, active, created_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(&body.service_id)
    .bind(&body.spec_url)
    .bind(&body.format)
    .bind(body.interval_minutes)
    .bind(&now)
    .execute(&pool)
    .await?;

    let row = sqlx::query(
        "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, active, created_at FROM scheduled_scan WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await?;

    Ok((StatusCode::CREATED, Json(row_to_response(&row))))
}

// ---------------------------------------------------------------------------
// GET /v1/scheduled-scans
// ---------------------------------------------------------------------------

pub(crate) async fn list_scans(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let rows = sqlx::query(
        "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, active, created_at FROM scheduled_scan WHERE org_id = ? ORDER BY created_at DESC",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let list: Vec<ScanResponse> = rows.iter().map(row_to_response).collect();
    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// DELETE /v1/scheduled-scans/:id
// ---------------------------------------------------------------------------

pub(crate) async fn delete_scan(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let row = sqlx::query("SELECT id FROM scheduled_scan WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .fetch_optional(&pool)
        .await?;
    if row.is_none() {
        return Err(ApiError::NotFound(format!("scheduled scan {id} not found")));
    }

    sqlx::query("DELETE FROM scheduled_scan WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .execute(&pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Background scanner task (K-3-T2)
// ---------------------------------------------------------------------------

/// Start the background loop that polls for due scheduled scans.
/// Call once from `run()` after the pool is initialised.
pub(crate) fn start_scan_scheduler(pool: sqlx::AnyPool) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            run_due_scans(pool.clone()).await;
        }
    });
}

async fn run_due_scans(pool: sqlx::AnyPool) {
    let now = Utc::now();

    // Fetch all active scans; due-time arithmetic is done in Rust to avoid
    // SQLite-only datetime() syntax (datetime(last_run_at, '+N minutes') is not
    // valid on PostgreSQL — the production deployment target).
    let rows = match sqlx::query(
        "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, last_spec_hash
         FROM scheduled_scan WHERE active = 1",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("scan scheduler: query failed: {e}");
            return;
        }
    };

    for row in rows {
        let interval_minutes: i32 = row.get("interval_minutes");
        let last_run_at: Option<String> = row.try_get("last_run_at").ok().flatten();

        let is_due = match &last_run_at {
            None => true,
            Some(ts) => match ts.parse::<DateTime<Utc>>() {
                Ok(last) => (now - last).num_minutes() >= i64::from(interval_minutes),
                Err(_) => true, // unparseable timestamp — treat as overdue
            },
        };

        if !is_due {
            continue;
        }

        let pool2 = pool.clone();
        let id: String = row.get("id");
        let org_id: String = row.get("org_id");
        let service_id: String = row.get("service_id");
        let spec_url: String = row.get("spec_url");
        let format: String = row.get("format");
        let last_hash: Option<String> = row.try_get("last_spec_hash").ok().flatten();
        tokio::spawn(async move {
            execute_scan(pool2, id, org_id, service_id, spec_url, format, last_hash).await;
        });
    }
}

async fn execute_scan(
    pool: sqlx::AnyPool,
    scan_id: String,
    org_id: String,
    service_id: String,
    spec_url: String,
    format: String,
    last_spec_hash: Option<String>,
) {
    let now = Utc::now().to_rfc3339();

    // Defense-in-depth SSRF check — spec_url is also validated at scan creation time,
    // but we re-check here in case records were inserted before the guard was added.
    if is_ssrf_blocked(&spec_url) {
        tracing::warn!("scan {scan_id}: SSRF-blocked spec_url — skipping execution");
        return;
    }

    // Mark run started
    let _ = sqlx::query("UPDATE scheduled_scan SET last_run_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&scan_id)
        .execute(&pool)
        .await;

    let http = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("radar-api/scheduled-scan")
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    let spec_text = match http.get(&spec_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("scan {scan_id}: failed to read response body: {e}");
                return;
            }
        },
        Ok(resp) => {
            tracing::warn!("scan {scan_id}: fetch returned HTTP {}", resp.status());
            return;
        }
        Err(e) => {
            tracing::warn!("scan {scan_id}: fetch error: {e}");
            return;
        }
    };

    let hash = format!("{:x}", Sha256::digest(spec_text.as_bytes()));

    // No change — nothing to diff
    if last_spec_hash.as_deref() == Some(hash.as_str()) {
        return;
    }

    // Fetch previous spec text if we have a hash (means there's a stored version)
    let base_spec = if last_spec_hash.is_some() {
        let row = sqlx::query(
            "SELECT spec_yaml FROM spec_version WHERE service_id = ? ORDER BY captured_at DESC LIMIT 1 OFFSET 1",
        )
        .bind(&service_id)
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
        row.and_then(|r| r.try_get::<String, _>("spec_yaml").ok()).unwrap_or_default()
    } else {
        // First run — just store the spec, no diff to create
        let _ = sqlx::query("UPDATE scheduled_scan SET last_spec_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&scan_id)
            .execute(&pool)
            .await;
        // Still need to store the spec version
        store_scan_spec(&pool, &service_id, &org_id, &spec_url, &format, &spec_text, &now).await;
        return;
    };

    // Store new spec version
    store_scan_spec(&pool, &service_id, &org_id, &spec_url, &format, &spec_text, &now).await;

    // Run diff via compare_specs logic
    if let Some(diff_id) = create_scan_diff(
        &pool,
        &service_id,
        &org_id,
        &format,
        &base_spec,
        &spec_text,
        &now,
    )
    .await
    {
        // Update hash and fire webhooks
        let _ = sqlx::query("UPDATE scheduled_scan SET last_spec_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&scan_id)
            .execute(&pool)
            .await;
        dispatch_diff_event(pool, diff_id, org_id).await;
    } else {
        let _ = sqlx::query("UPDATE scheduled_scan SET last_spec_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&scan_id)
            .execute(&pool)
            .await;
    }
}

async fn store_scan_spec(
    pool: &sqlx::AnyPool,
    service_id: &str,
    org_id: &str,
    spec_url: &str,
    format: &str,
    spec_text: &str,
    now: &str,
) {
    let svc_id = service_id.to_string();
    let _ = sqlx::query(
        "INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) VALUES (?, ?, '', '', ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&svc_id)
    .bind(&svc_id)
    .bind(format)
    .bind(org_id)
    .execute(pool)
    .await;

    let version_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{svc_id}:{spec_url}:{now}").as_bytes(),
    )
    .to_string();

    let _ = sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&version_id)
    .bind(&svc_id)
    .bind(spec_url)
    .bind(now)
    .bind(format)
    .bind(spec_text)
    .execute(pool)
    .await;
}

async fn create_scan_diff(
    pool: &sqlx::AnyPool,
    service_id: &str,
    _org_id: &str,
    format: &str,
    base_spec: &str,
    head_spec: &str,
    now: &str,
) -> Option<String> {
    let changes: Vec<radar_core::diff::DiffChange> = match format.to_lowercase().as_str() {
        "graphql" | "gql" => {
            let base = radar_core::graphql::parse_graphql(base_spec).ok()?;
            let head = radar_core::graphql::parse_graphql(head_spec).ok()?;
            radar_core::graphql::diff_graphql(&base, &head)
        }
        "protobuf" | "proto" => {
            let base = radar_core::proto::parse_proto(base_spec).ok()?;
            let head = radar_core::proto::parse_proto(head_spec).ok()?;
            radar_core::proto::diff_proto(&base, &head)
        }
        _ => {
            let base = radar_core::diff::parse_openapi(base_spec).ok()?;
            let head = radar_core::diff::parse_openapi(head_spec).ok()?;
            radar_core::diff::diff_openapi(&base, &head)
        }
    };

    if changes.is_empty() {
        return None;
    }

    let from_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{service_id}:scan:base:{now}").as_bytes(),
    )
    .to_string();
    let to_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{service_id}:scan:head:{now}").as_bytes(),
    )
    .to_string();

    let _ = sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) VALUES (?, ?, 'scan:base', ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&from_id)
    .bind(service_id)
    .bind(now)
    .bind(format)
    .bind(base_spec)
    .execute(pool)
    .await;

    let _ = sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) VALUES (?, ?, 'scan:head', ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&to_id)
    .bind(service_id)
    .bind(now)
    .bind(format)
    .bind(head_spec)
    .execute(pool)
    .await;

    let diff_id = Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, NULL, ?)",
    )
    .bind(&diff_id)
    .bind(&from_id)
    .bind(&to_id)
    .bind(now)
    .execute(pool)
    .await;

    for change in &changes {
        let change_id = Uuid::new_v4().to_string();
        let sev = match change.severity {
            radar_core::models::Severity::Breaking => "breaking",
            radar_core::models::Severity::NonBreakingRisky => "non_breaking_risky",
            radar_core::models::Severity::Safe => "safe",
        };
        let _ = sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&change_id)
        .bind(&diff_id)
        .bind(&change.path)
        .bind(change.kind.as_str())
        .bind(sev)
        .bind(&change.description)
        .execute(pool)
        .await;
    }

    metrics::counter!("radar_diffs_created_total").increment(1);
    Some(diff_id)
}

// GET /v1/scheduled-scans/run-history (basic: last_run_at per scan)
pub(crate) async fn run_history(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    // NULLS LAST is PostgreSQL-only syntax; use CASE to sort nulls last on both SQLite and PostgreSQL.
    let rows = sqlx::query(
        "SELECT id, service_id, spec_url, last_run_at, last_spec_hash FROM scheduled_scan WHERE org_id = ? ORDER BY CASE WHEN last_run_at IS NULL THEN 1 ELSE 0 END ASC, last_run_at DESC",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let history: Vec<Value> = rows.iter().map(|r| json!({
        "id": r.get::<String,_>("id"),
        "service_id": r.get::<String,_>("service_id"),
        "spec_url": r.get::<String,_>("spec_url"),
        "last_run_at": r.try_get::<String,_>("last_run_at").ok(),
        "last_spec_hash": r.try_get::<String,_>("last_spec_hash").ok(),
    })).collect();

    Ok(Json(history))
}
