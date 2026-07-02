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
use crate::utils::{is_host_allowed, is_ssrf_blocked};
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

fn default_format() -> String {
    "openapi".to_string()
}
fn default_interval() -> i32 {
    60
}

#[derive(Serialize)]
struct ScanResponse {
    id: String,
    org_id: String,
    service_id: String,
    spec_url: String,
    format: String,
    interval_minutes: i32,
    last_run_at: Option<String>,
    last_run_status: Option<String>,
    last_run_error: Option<String>,
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
        last_run_at: row.try_get("last_run_at").ok().flatten(),
        last_run_status: row.try_get("last_run_status").ok().flatten(),
        last_run_error: row.try_get("last_run_error").ok().flatten(),
        active: {
            let v: i32 = row.get("active");
            v != 0
        },
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
        return Err(ApiError::BadRequest(
            "interval_minutes must be at least 15".into(),
        ));
    }
    if body.service_id.is_empty() {
        return Err(ApiError::BadRequest("service_id is required".into()));
    }
    // Org isolation (authz before any outbound processing): cannot schedule a
    // scan for another org's service.
    crate::auth::require_org_owned(
        &pool,
        crate::auth::OrgResource::Service,
        &body.service_id,
        &org_id,
    )
    .await?;
    if body.spec_url.is_empty() {
        return Err(ApiError::BadRequest("spec_url is required".into()));
    }
    if is_ssrf_blocked(&body.spec_url) || !is_host_allowed(&body.spec_url) {
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
        sqlx::query("UPDATE scheduled_scan SET interval_minutes = ?, active = 1 WHERE id = ?")
            .bind(body.interval_minutes)
            .bind(&id)
            .execute(&pool)
            .await?;
        let updated = sqlx::query(
            "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, last_run_status, last_run_error, active, created_at FROM scheduled_scan WHERE id = ?",
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
        "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, last_run_status, last_run_error, active, created_at FROM scheduled_scan WHERE id = ?",
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
        "SELECT id, org_id, service_id, spec_url, format, interval_minutes, last_run_at, last_run_status, last_run_error, active, created_at FROM scheduled_scan WHERE org_id = ? ORDER BY created_at DESC",
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

    // Defense-in-depth SSRF + allowlist check.
    if is_ssrf_blocked(&spec_url) || !is_host_allowed(&spec_url) {
        tracing::warn!("scan {scan_id}: SSRF-blocked or disallowed spec_url — skipping execution");
        set_scan_status(
            &pool,
            &scan_id,
            &now,
            "skipped",
            Some("SSRF-blocked or disallowed spec_url"),
        )
        .await;
        return;
    }

    // Mark run started (status clears previous error to 'running').
    let _ = sqlx::query(
        "UPDATE scheduled_scan SET last_run_at = ?, last_run_status = 'running', last_run_error = NULL WHERE id = ?",
    )
    .bind(&now)
    .bind(&scan_id)
    .execute(&pool)
    .await;

    let http = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("radar-api/scheduled-scan")
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            set_scan_status(&pool, &scan_id, &now, "failed", Some(&e.to_string())).await;
            return;
        }
    };

    let spec_text = match http.get(&spec_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                let msg = format!("failed to read response body: {e}");
                tracing::warn!("scan {scan_id}: {msg}");
                set_scan_status(&pool, &scan_id, &now, "failed", Some(&msg)).await;
                crate::audit::record_event(
                    &pool,
                    &org_id,
                    "system",
                    "scan.run.failed",
                    Some("scheduled_scan"),
                    Some(&scan_id),
                    Some(&serde_json::json!({ "error": msg })),
                )
                .await;
                return;
            }
        },
        Ok(resp) => {
            let msg = format!("HTTP {}", resp.status().as_u16());
            tracing::warn!("scan {scan_id}: fetch returned {msg}");
            set_scan_status(&pool, &scan_id, &now, "failed", Some(&msg)).await;
            crate::audit::record_event(
                &pool,
                &org_id,
                "system",
                "scan.run.failed",
                Some("scheduled_scan"),
                Some(&scan_id),
                Some(&serde_json::json!({ "error": msg })),
            )
            .await;
            return;
        }
        Err(e) => {
            let msg = format!("fetch error: {e}");
            tracing::warn!("scan {scan_id}: {msg}");
            set_scan_status(&pool, &scan_id, &now, "failed", Some(&msg)).await;
            crate::audit::record_event(
                &pool,
                &org_id,
                "system",
                "scan.run.failed",
                Some("scheduled_scan"),
                Some(&scan_id),
                Some(&serde_json::json!({ "error": msg })),
            )
            .await;
            return;
        }
    };

    let hash = format!("{:x}", Sha256::digest(spec_text.as_bytes()));

    // No change — mark ok and done.
    if last_spec_hash.as_deref() == Some(hash.as_str()) {
        set_scan_status(&pool, &scan_id, &now, "ok", None).await;
        crate::audit::record_event(
            &pool,
            &org_id,
            "system",
            "scan.run.completed",
            Some("scheduled_scan"),
            Some(&scan_id),
            Some(&serde_json::json!({ "changed": false })),
        )
        .await;
        return;
    }

    // Fetch previous spec text if we have a hash (means there's a stored version).
    // The new spec has NOT been stored yet at this point, so the most recent
    // stored version (OFFSET 0) IS the previous spec — an earlier `OFFSET 1`
    // skipped it, diffing against an empty or two-generations-old spec.
    let base_spec = if last_spec_hash.is_some() {
        fetch_previous_spec(&pool, &service_id).await
    } else {
        // First run — store the spec, no diff to create yet.
        let _ = sqlx::query("UPDATE scheduled_scan SET last_spec_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&scan_id)
            .execute(&pool)
            .await;
        store_scan_spec(
            &pool,
            &service_id,
            &org_id,
            &spec_url,
            &format,
            &spec_text,
            &now,
        )
        .await;
        set_scan_status(&pool, &scan_id, &now, "ok", None).await;
        crate::audit::record_event(
            &pool,
            &org_id,
            "system",
            "scan.run.completed",
            Some("scheduled_scan"),
            Some(&scan_id),
            Some(&serde_json::json!({ "changed": false, "first_run": true })),
        )
        .await;
        return;
    };

    // Store new spec version.
    store_scan_spec(
        &pool,
        &service_id,
        &org_id,
        &spec_url,
        &format,
        &spec_text,
        &now,
    )
    .await;

    // Run diff.
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
        let _ = sqlx::query("UPDATE scheduled_scan SET last_spec_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&scan_id)
            .execute(&pool)
            .await;
        set_scan_status(&pool, &scan_id, &now, "ok", None).await;
        crate::audit::record_event(
            &pool,
            &org_id,
            "system",
            "scan.run.completed",
            Some("scheduled_scan"),
            Some(&scan_id),
            Some(&serde_json::json!({ "changed": true, "diff_id": diff_id })),
        )
        .await;
        dispatch_diff_event(pool, diff_id, org_id).await;
    } else {
        let _ = sqlx::query("UPDATE scheduled_scan SET last_spec_hash = ? WHERE id = ?")
            .bind(&hash)
            .bind(&scan_id)
            .execute(&pool)
            .await;
        set_scan_status(&pool, &scan_id, &now, "ok", None).await;
        crate::audit::record_event(
            &pool,
            &org_id,
            "system",
            "scan.run.completed",
            Some("scheduled_scan"),
            Some(&scan_id),
            Some(&serde_json::json!({ "changed": false })),
        )
        .await;
    }
}

/// Return the most recently stored spec YAML for a service (the previous spec,
/// since the current scan's spec is stored only after this is called). Ordering
/// is `captured_at DESC, id DESC` so that identical timestamps break
/// deterministically instead of relying on physical row order.
async fn fetch_previous_spec(pool: &sqlx::AnyPool, service_id: &str) -> String {
    let row = sqlx::query(
        "SELECT spec_yaml FROM spec_version WHERE service_id = ? ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(service_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    row.and_then(|r| r.try_get::<String, _>("spec_yaml").ok())
        .unwrap_or_default()
}

async fn set_scan_status(
    pool: &sqlx::AnyPool,
    scan_id: &str,
    run_at: &str,
    status: &str,
    error: Option<&str>,
) {
    let _ = sqlx::query(
        "UPDATE scheduled_scan SET last_run_at = ?, last_run_status = ?, last_run_error = ? WHERE id = ?",
    )
    .bind(run_at)
    .bind(status)
    .bind(error)
    .bind(scan_id)
    .execute(pool)
    .await;
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
    let change_rows: Vec<crate::diffs::ChangeInsert> = changes
        .iter()
        .map(crate::diffs::ChangeInsert::from_diff)
        .collect();
    let final_diff_id = match crate::diffs::persist_diff_atomic(
        pool,
        &diff_id,
        &from_id,
        &to_id,
        None,
        now,
        &change_rows,
    )
    .await
    {
        Ok(crate::diffs::DiffWriteOutcome::Created) => diff_id,
        Ok(crate::diffs::DiffWriteOutcome::AlreadyExists(existing)) => existing,
        Err(e) => {
            tracing::warn!("scan diff persist failed: {e}");
            return None;
        }
    };

    metrics::counter!("radar_diffs_created_total").increment(1);
    Some(final_diff_id)
}

// GET /v1/scheduled-scans/run-history (basic: last_run_at per scan)
pub(crate) async fn run_history(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    // NULLS LAST is PostgreSQL-only syntax; use CASE to sort nulls last on both SQLite and PostgreSQL.
    let rows = sqlx::query(
        "SELECT id, service_id, spec_url, last_run_at, last_run_status, last_run_error, last_spec_hash \
         FROM scheduled_scan WHERE org_id = ? \
         ORDER BY CASE WHEN last_run_at IS NULL THEN 1 ELSE 0 END ASC, last_run_at DESC",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let history: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<String,_>("id"),
                "service_id": r.get::<String,_>("service_id"),
                "spec_url": r.get::<String,_>("spec_url"),
                "last_run_at": r.try_get::<Option<String>,_>("last_run_at").ok().flatten(),
                "last_run_status": r.try_get::<Option<String>,_>("last_run_status").ok().flatten(),
                "last_run_error": r.try_get::<Option<String>,_>("last_run_error").ok().flatten(),
                "last_spec_hash": r.try_get::<Option<String>,_>("last_spec_hash").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(history))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::fetch_previous_spec;

    async fn test_pool() -> sqlx::AnyPool {
        sqlx::any::install_default_drivers();
        let url = crate::test_helpers::test_db_url();
        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("pool");
        crate::test_helpers::isolate_postgres_schema(&pool, &url).await;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrate");
        if url.starts_with("sqlite") {
            sqlx::query("PRAGMA foreign_keys = OFF")
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn insert_spec(
        pool: &sqlx::AnyPool,
        id: &str,
        service_id: &str,
        captured_at: &str,
        yaml: &str,
    ) {
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) \
             VALUES (?, ?, 'test', ?, 'openapi', ?)",
        )
        .bind(id)
        .bind(service_id)
        .bind(captured_at)
        .bind(yaml)
        .execute(pool)
        .await
        .expect("insert spec_version");
    }

    // M-6: the previous spec is the most recently stored version (OFFSET 0),
    // not the one before it. With two stored versions, fetch_previous_spec must
    // return the newer one — the base a not-yet-stored 3rd scan diffs against.
    #[tokio::test]
    async fn fetch_previous_spec_returns_most_recent_stored() {
        let pool = test_pool().await;
        let svc = "svc-m6";
        insert_spec(
            &pool,
            "v1",
            svc,
            "2026-07-01T10:00:00.000000000+00:00",
            "spec-v1",
        )
        .await;
        insert_spec(
            &pool,
            "v2",
            svc,
            "2026-07-02T10:00:00.000000000+00:00",
            "spec-v2",
        )
        .await;

        let base = fetch_previous_spec(&pool, svc).await;
        assert_eq!(
            base, "spec-v2",
            "must diff against the immediately previous spec, not an older one"
        );
    }

    #[tokio::test]
    async fn fetch_previous_spec_empty_when_none_stored() {
        let pool = test_pool().await;
        assert_eq!(fetch_previous_spec(&pool, "nobody").await, "");
    }
}
