use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::auth::CallerOrg;
use crate::errors::ApiError;
use crate::utils::{is_host_allowed, is_ssrf_blocked};

// Thread-local SSRF bypass for in-process echo-server tests.
// Compiled out of non-test builds; never reachable in production.
#[cfg(test)]
thread_local! {
    static SSRF_BYPASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum response body stored per row when `capture_body` is enabled (O-5).
const BODY_CAPTURE_LIMIT: usize = 10 * 1024;

/// Per-row retry policy: 3 attempts, delays 0s → 1s → 4s (O-4).
const CSV_ROW_MAX_ATTEMPTS: u8 = 3;
const CSV_ROW_RETRY_DELAYS_SECS: [u64; 3] = [0, 1, 4];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct CreateCsvRunBody {
    #[serde(default)]
    name: String,
    request: RequestTemplate,
    rows: Vec<serde_json::Value>,
}

#[derive(Deserialize, serde::Serialize, Clone)]
pub(crate) struct RequestTemplate {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<HeaderPair>,
    #[serde(default)]
    pub body: String,
    /// When true the first BODY_CAPTURE_LIMIT bytes of each successful response
    /// body are stored in csv_run_result.response_body (O-5).
    #[serde(default)]
    pub capture_body: bool,
    /// When true, rows are retried on 5xx / network errors (up to CSV_ROW_MAX_ATTEMPTS).
    /// Defaults to false — callers should opt in for idempotent targets only.
    /// Safe methods (GET, HEAD) always get the full retry budget regardless of this flag.
    #[serde(default)]
    pub enable_retry: bool,
}

#[derive(Deserialize, serde::Serialize, Clone)]
pub(crate) struct HeaderPair {
    pub key: String,
    pub value: String,
}

fn default_method() -> String {
    "GET".to_string()
}

#[derive(Deserialize)]
pub(crate) struct ResultsQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    500
}

struct RowOutcome {
    row_number: i64,
    http_status: Option<i64>,
    duration_ms: i64,
    error: Option<String>,
    url: String,
    response_body: Option<String>,
    /// Serialised JSON of the original input row, for server-side failed-row export.
    row_data: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

// POST /v1/csv-runs
pub(crate) async fn create_csv_run(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Json(body): Json<CreateCsvRunBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.rows.is_empty() {
        return Err(ApiError::Unprocessable("rows must not be empty".into()));
    }
    if body.rows.len() > 500 {
        return Err(ApiError::Unprocessable(
            "rows exceeds maximum of 500".into(),
        ));
    }
    if body.request.url.trim().is_empty() {
        return Err(ApiError::Unprocessable("request.url is required".into()));
    }

    // Template-level SSRF check — only when the host position is concrete.
    // If the host is itself a placeholder ("https://{{host}}/api") we skip
    // the check here and let the per-row guard handle it.
    if template_url_has_concrete_host(&body.request.url) {
        let stripped = strip_placeholders(&body.request.url);
        #[cfg(not(test))]
        let ssrf_bypass = false;
        #[cfg(test)]
        let ssrf_bypass = SSRF_BYPASS.with(|v| v.get());
        if !ssrf_bypass && (is_ssrf_blocked(&stripped) || !is_host_allowed(&stripped)) {
            return Err(ApiError::Unprocessable(
                "request URL is blocked by SSRF policy".into(),
            ));
        }
    }

    let org_id = caller.sql_scope().to_string();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let total_rows = body.rows.len() as i64;

    let request_json = serde_json::to_string(&body.request)
        .map_err(|_| ApiError::Unprocessable("failed to serialize request template".into()))?;

    q!(
        "INSERT INTO csv_run_job \
         (id, org_id, name, request_json, status, total_rows, completed_rows, error_count, created_at) \
         VALUES (?, ?, ?, ?, 'pending', ?, 0, 0, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(body.name.trim())
    .bind(&request_json)
    .bind(total_rows)
    .bind(&now)
    .execute(&pool)
    .await?;

    {
        let pool2 = pool.clone();
        let oid = org_id.clone();
        let jid = id.clone();
        let n = total_rows;
        tokio::spawn(async move {
            crate::audit::record_event(
                &pool2,
                &oid,
                "system",
                "csv_run.started",
                Some("csv_run_job"),
                Some(&jid),
                Some(&serde_json::json!({ "total_rows": n })),
            )
            .await;
        });
    }

    let pool2 = pool.clone();
    let job_id = id.clone();
    let template = body.request.clone();
    let rows = body.rows.clone();
    tokio::spawn(async move {
        execute_csv_run(pool2, job_id, template, rows).await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "id": id,
            "status": "pending",
            "total_rows": total_rows,
        })),
    ))
}

// GET /v1/csv-runs
pub(crate) async fn list_csv_runs(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();

    let rows = q!(
        "SELECT id, name, status, total_rows, completed_rows, error_count, error_message, \
                created_at, started_at, completed_at \
         FROM csv_run_job WHERE org_id = ? ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let items: Vec<serde_json::Value> = rows.iter().map(row_to_job_json).collect();
    Ok(Json(items))
}

// GET /v1/csv-runs/:id
pub(crate) async fn get_csv_run(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();

    let row = q!(
        "SELECT id, name, status, total_rows, completed_rows, error_count, error_message, \
                created_at, started_at, completed_at \
         FROM csv_run_job WHERE id = ? AND org_id = ?",
    )
    .bind(&id)
    .bind(&org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("csv run not found".into()))?;

    Ok(Json(row_to_job_json(&row)))
}

// DELETE /v1/csv-runs/:id
pub(crate) async fn cancel_csv_run(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();

    let result = q!("UPDATE csv_run_job SET status = 'cancelled' \
         WHERE id = ? AND org_id = ? AND status IN ('pending', 'running')",)
    .bind(&id)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(
            "csv run not found or already complete".into(),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// GET /v1/csv-runs/:id/results
pub(crate) async fn get_csv_run_results(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Path(id): Path<String>,
    Query(params): Query<ResultsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();

    let exists = q!("SELECT 1 FROM csv_run_job WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .fetch_optional(&pool)
        .await?;

    if exists.is_none() {
        return Err(ApiError::NotFound("csv run not found".into()));
    }

    let rows = q!(
        "SELECT row_number, http_status, duration_ms, error, url, response_body, row_data \
         FROM csv_run_result WHERE job_id = ? ORDER BY row_number ASC \
         LIMIT ? OFFSET ?",
    )
    .bind(&id)
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(&pool)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "row_number":     r.try_get::<i64, _>("row_number").unwrap_or(0),
                "http_status":    r.try_get::<Option<i64>, _>("http_status").unwrap_or(None),
                "duration_ms":    r.try_get::<i64, _>("duration_ms").unwrap_or(0),
                "error":          r.try_get::<Option<String>, _>("error").unwrap_or(None),
                "url":            r.try_get::<String, _>("url").unwrap_or_default(),
                "response_body":  r.try_get::<Option<String>, _>("response_body").unwrap_or(None),
                "row_data":       r.try_get::<Option<String>, _>("row_data").unwrap_or(None),
            })
        })
        .collect();

    Ok(Json(items))
}

// ---------------------------------------------------------------------------
// Background executor — orchestrator
// ---------------------------------------------------------------------------

#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn execute_csv_run(
    pool: sqlx::AnyPool,
    job_id: String,
    template: RequestTemplate,
    rows: Vec<serde_json::Value>,
) {
    set_job_running(&pool, &job_id).await;
    let client = build_http_client();
    let (completed, error_count, executor_failed) =
        run_rows(&pool, &client, &job_id, &template, &rows).await;
    finalize_job(&pool, &job_id, completed, error_count, executor_failed).await;
}

async fn set_job_running(pool: &sqlx::AnyPool, job_id: &str) {
    let started_at = Utc::now().to_rfc3339();
    let _ = q!("UPDATE csv_run_job SET status = 'running', started_at = ? WHERE id = ?")
        .bind(&started_at)
        .bind(job_id)
        .execute(pool)
        .await;
}

/// Fallback client, used only on the test SSRF-bypass path where the target is
/// an in-process echo server on 127.0.0.1 and there is nothing to pin.
///
/// Production requests go through [`crate::utils::ssrf_pinned_client`] instead
/// — see the per-row pin in `dispatch_row`.
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

async fn run_rows(
    pool: &sqlx::AnyPool,
    client: &reqwest::Client,
    job_id: &str,
    template: &RequestTemplate,
    rows: &[serde_json::Value],
) -> (i64, i64, bool) {
    let mut completed = 0i64;
    let mut error_count = 0i64;

    for (i, row) in rows.iter().enumerate() {
        if job_is_cancelled(pool, job_id).await {
            break;
        }
        let row_number = (i + 1) as i64;
        let outcome = dispatch_row(pool, job_id, client, template, row, row_number).await;
        // Count any HTTP error (4xx/5xx) or network failure as an error, not just
        // network-only failures. A 404 is a failed row even though error field is None.
        if outcome.error.is_some() || outcome.http_status.is_none_or(|s| s >= 400) {
            error_count += 1;
        }
        insert_result(pool, job_id, &outcome).await;
        completed += 1;
        if update_progress(pool, job_id, completed, error_count)
            .await
            .is_err()
        {
            tracing::warn!(job_id = %job_id, "failed to update job progress, aborting executor");
            return (completed, error_count, true);
        }
    }
    (completed, error_count, false)
}

async fn finalize_job(
    pool: &sqlx::AnyPool,
    job_id: &str,
    completed: i64,
    error_count: i64,
    executor_failed: bool,
) {
    let status = if executor_failed {
        "failed"
    } else if error_count > 0 {
        "completed_with_failures"
    } else {
        "completed"
    };
    let completed_at = Utc::now().to_rfc3339();
    let _ = q!("UPDATE csv_run_job \
         SET status = ?, completed_at = ?, completed_rows = ?, error_count = ? \
         WHERE id = ? AND status NOT IN ('cancelled')",)
    .bind(status)
    .bind(&completed_at)
    .bind(completed)
    .bind(error_count)
    .bind(job_id)
    .execute(pool)
    .await;
}

// ---------------------------------------------------------------------------
// Row dispatch — SSRF guard, retry loop (O-4), single-attempt helper
// ---------------------------------------------------------------------------

async fn dispatch_row(
    pool: &sqlx::AnyPool,
    job_id: &str,
    client: &reqwest::Client,
    template: &RequestTemplate,
    row: &serde_json::Value,
    row_number: i64,
) -> RowOutcome {
    let row_data = serde_json::to_string(row).unwrap_or_default();

    let row_obj = match row.as_object() {
        Some(o) => o,
        None => {
            return RowOutcome {
                row_number,
                http_status: None,
                duration_ms: 0,
                error: Some("row is not an object".into()),
                url: String::new(),
                response_body: None,
                row_data,
            }
        }
    };

    let resolved_url = resolve_vars(&template.url, row_obj);
    // In test builds: SSRF_BYPASS thread-local allows in-process echo servers at
    // 127.0.0.1 to receive requests.  Compiled out of production builds entirely.
    #[cfg(not(test))]
    let ssrf_bypass = false;
    #[cfg(test)]
    let ssrf_bypass = SSRF_BYPASS.with(|v| v.get());
    if !ssrf_bypass && (is_ssrf_blocked(&resolved_url) || !is_host_allowed(&resolved_url)) {
        return RowOutcome {
            row_number,
            http_status: None,
            duration_ms: 0,
            error: Some("URL blocked by SSRF policy".into()),
            url: resolved_url,
            response_body: None,
            row_data,
        };
    }

    const ALLOWED_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];
    let method_upper = template.method.to_uppercase();
    if !ALLOWED_METHODS.contains(&method_upper.as_str()) {
        return RowOutcome {
            row_number,
            http_status: None,
            duration_ms: 0,
            error: Some(format!("unsupported HTTP method: {}", template.method)),
            url: resolved_url,
            response_body: None,
            row_data,
        };
    }

    // Safe methods (GET, HEAD) always retry; other methods only if opted in.
    let is_safe_method = matches!(method_upper.as_str(), "GET" | "HEAD");
    let max_attempts = if is_safe_method || template.enable_retry {
        CSV_ROW_MAX_ATTEMPTS
    } else {
        1
    };

    let resolved_body = resolve_vars(&template.body, row_obj);
    let start = std::time::Instant::now();
    let mut last_http_status: Option<i64> = None;
    let mut last_error: Option<String> = None;

    for attempt in 0u8..max_attempts {
        if attempt > 0 {
            if job_is_cancelled(pool, job_id).await {
                last_error = last_error.or(Some("cancelled".into()));
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(
                CSV_ROW_RETRY_DELAYS_SECS[attempt as usize],
            ))
            .await;
        }
        match build_and_send(
            client,
            &template.method,
            &resolved_url,
            &template.headers,
            row_obj,
            &resolved_body,
            template.capture_body,
        )
        .await
        {
            Ok((status, _)) if status >= 500 => {
                last_http_status = Some(status as i64);
                last_error = Some(format!("HTTP {status}"));
            }
            Ok((status, body)) => {
                return RowOutcome {
                    row_number,
                    http_status: Some(status as i64),
                    duration_ms: start.elapsed().as_millis() as i64,
                    error: None,
                    url: resolved_url,
                    response_body: body,
                    row_data,
                };
            }
            Err(e) => {
                last_error = Some(e.to_string());
            }
        }
    }

    RowOutcome {
        row_number,
        http_status: last_http_status,
        duration_ms: start.elapsed().as_millis() as i64,
        error: last_error,
        url: resolved_url,
        response_body: None,
        row_data,
    }
}

/// Make one HTTP attempt. Retries must be handled by the caller.
/// Returns `(status_code, captured_body)` on any HTTP response.
/// Returns `Err` only on a network/connection-level failure.
async fn build_and_send(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    headers: &[HeaderPair],
    row: &serde_json::Map<String, serde_json::Value>,
    body: &str,
    capture_body: bool,
) -> Result<(u16, Option<String>), reqwest::Error> {
    // F-08: pin to the addresses just approved for THIS url. Re-resolving the
    // name at connect time is the rebinding window; a per-row client with a DNS
    // override closes it. On the test bypass path there is nothing to pin, so
    // the shared client is used as-is.
    #[cfg(not(test))]
    let pinned = crate::utils::ssrf_pinned_client(url, std::time::Duration::from_secs(10), None);
    #[cfg(test)]
    let pinned = if SSRF_BYPASS.with(|v| v.get()) {
        None
    } else {
        crate::utils::ssrf_pinned_client(url, std::time::Duration::from_secs(10), None)
    };
    let client = pinned.as_ref().unwrap_or(client);

    let req = build_request(client, method, url, headers, row, body);
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    // Only read body on non-5xx responses: 5xx will be retried so consuming
    // the body here would not help and costs a read on a connection we'll discard.
    let response_body = if capture_body && status < 500 {
        let bytes = resp.bytes().await.unwrap_or_default();
        let limited: Vec<u8> = bytes.into_iter().take(BODY_CAPTURE_LIMIT).collect();
        Some(String::from_utf8_lossy(&limited).into_owned())
    } else {
        None
    };
    Ok((status, response_body))
}

fn build_request(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    headers: &[HeaderPair],
    row: &serde_json::Map<String, serde_json::Value>,
    body: &str,
) -> reqwest::RequestBuilder {
    let m = method.to_uppercase();
    let mut builder = match m.as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        "HEAD" => client.head(url),
        _ => client.get(url), // unreachable after dispatch_row method guard
    };
    for h in headers {
        let key = resolve_vars(&h.key, row);
        let val = resolve_vars(&h.value, row);
        if !key.is_empty() {
            builder = builder.header(&key, &val);
        }
    }
    if !body.is_empty() && !matches!(m.as_str(), "GET" | "HEAD") {
        builder = builder.body(body.to_string());
    }
    builder
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

async fn job_is_cancelled(pool: &sqlx::AnyPool, job_id: &str) -> bool {
    q!("SELECT status FROM csv_run_job WHERE id = ?")
        .bind(job_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String, _>("status").ok())
        .map(|s| s == "cancelled")
        .unwrap_or(false)
}

/// Returns true when the scheme+host are concrete, i.e. the character
/// immediately after "https://" or "http://" is not a `{{` placeholder.
/// Templates like `https://{{hostname}}/api` return false so that the
/// template-level SSRF check is skipped and per-row guards handle it instead.
fn template_url_has_concrete_host(url: &str) -> bool {
    for prefix in ["https://", "http://"] {
        if let Some(rest) = url.strip_prefix(prefix) {
            return !rest.starts_with("{{");
        }
    }
    false
}

fn strip_placeholders(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'}' && bytes[i + 1] == b'}') {
                i += 1;
            }
            i += 2;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn resolve_vars(template: &str, row: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut result = template.to_string();
    for (key, value) in row {
        let placeholder = format!("{{{{{}}}}}", key);
        let val_str = match value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        result = result.replace(&placeholder, &val_str);
    }
    result
}

async fn insert_result(pool: &sqlx::AnyPool, job_id: &str, r: &RowOutcome) {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    if let Err(e) = q!(
        "INSERT INTO csv_run_result \
         (id, job_id, row_number, http_status, duration_ms, error, url, response_body, row_data, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(job_id)
    .bind(r.row_number)
    .bind(r.http_status)
    .bind(r.duration_ms)
    .bind(r.error.as_deref())
    .bind(&r.url)
    .bind(r.response_body.as_deref())
    .bind(&r.row_data)
    .bind(&now)
    .execute(pool)
    .await
    {
        tracing::warn!(job_id = %job_id, row_number = r.row_number, error = %e, "failed to write csv_run_result");
    }
}

async fn update_progress(
    pool: &sqlx::AnyPool,
    job_id: &str,
    completed: i64,
    error_count: i64,
) -> Result<(), sqlx::Error> {
    q!("UPDATE csv_run_job SET completed_rows = ?, error_count = ? WHERE id = ?")
        .bind(completed)
        .bind(error_count)
        .bind(job_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn row_to_job_json(r: &sqlx::any::AnyRow) -> serde_json::Value {
    json!({
        "id":             r.try_get::<String, _>("id").unwrap_or_default(),
        "name":           r.try_get::<String, _>("name").unwrap_or_default(),
        "status":         r.try_get::<String, _>("status").unwrap_or_default(),
        "total_rows":     r.try_get::<i64, _>("total_rows").unwrap_or(0),
        "completed_rows": r.try_get::<i64, _>("completed_rows").unwrap_or(0),
        "error_count":    r.try_get::<i64, _>("error_count").unwrap_or(0),
        "error_message":  r.try_get::<Option<String>, _>("error_message").unwrap_or(None),
        "created_at":     r.try_get::<String, _>("created_at").unwrap_or_default(),
        "started_at":     r.try_get::<Option<String>, _>("started_at").unwrap_or(None),
        "completed_at":   r.try_get::<Option<String>, _>("completed_at").unwrap_or(None),
    })
}

// ---------------------------------------------------------------------------
// Retention (O-3) — called from main.rs retention loop
// ---------------------------------------------------------------------------

/// Purge completed/failed/cancelled csv_run_job rows older than `days`.
/// csv_run_result rows cascade automatically via ON DELETE CASCADE.
/// Running/pending jobs are never purged.
pub async fn purge_old_csv_runs(pool: &sqlx::AnyPool, days: u32) -> Result<u64, sqlx::Error> {
    let cutoff = (Utc::now() - chrono::Duration::days(days as i64)).to_rfc3339();
    let result = q!(
        "DELETE FROM csv_run_job \
         WHERE status IN ('completed', 'completed_with_failures', 'failed', 'cancelled') AND created_at < ?",
    )
    .bind(&cutoff)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::purge_old_csv_runs;
    use axum::{body::Body, http::Request as HttpRequest};
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

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
            q!("PRAGMA foreign_keys = OFF")
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    async fn test_app() -> axum::Router {
        let pool = test_pool().await;
        crate::build_router(pool, None, 4 * 1024 * 1024, false, None)
    }

    // --- existing csv-run handler tests ---

    #[tokio::test]
    async fn post_csv_runs_empty_rows_returns_422() {
        let app = test_app().await;
        let body = serde_json::json!({
            "request": { "url": "https://httpbin.org/get", "method": "GET", "headers": [], "body": "" },
            "rows": []
        });
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/csv-runs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn post_csv_runs_too_many_rows_returns_422() {
        let app = test_app().await;
        let rows: Vec<serde_json::Value> =
            (0..501).map(|i| serde_json::json!({ "id": i })).collect();
        let body = serde_json::json!({
            "request": { "url": "https://httpbin.org/get", "method": "GET", "headers": [], "body": "" },
            "rows": rows
        });
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/csv-runs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn post_csv_runs_hardcoded_ssrf_url_returns_422() {
        let app = test_app().await;
        let body = serde_json::json!({
            "request": { "url": "https://192.168.1.1/api", "method": "GET", "headers": [], "body": "" },
            "rows": [{ "id": "1" }]
        });
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/csv-runs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    // A template where the hostname is itself a {{variable}} must not be
    // rejected at creation time — the per-row SSRF guard handles it.
    #[tokio::test]
    async fn post_csv_runs_variable_hostname_not_blocked() {
        let app = test_app().await;
        let body = serde_json::json!({
            "request": { "url": "https://{{hostname}}/api", "method": "GET", "headers": [], "body": "" },
            "rows": [{ "hostname": "api.example.com" }]
        });
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/csv-runs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn get_csv_run_unknown_id_returns_404() {
        let app = test_app().await;
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/csv-runs/nonexistent-id")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_csv_runs_returns_empty_array() {
        let app = test_app().await;
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/csv-runs")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn delete_csv_run_unknown_id_returns_404() {
        let app = test_app().await;
        let req = HttpRequest::builder()
            .method("DELETE")
            .uri("/v1/csv-runs/nonexistent-id")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
    }

    // capture_body flag accepted at creation (O-5)
    #[tokio::test]
    async fn post_csv_runs_with_capture_body_returns_202() {
        let app = test_app().await;
        let body = serde_json::json!({
            "request": {
                "url": "https://{{host}}/api",
                "method": "GET",
                "headers": [],
                "body": "",
                "capture_body": true
            },
            "rows": [{ "host": "api.example.com" }]
        });
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/csv-runs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::ACCEPTED);
    }

    // --- retention tests (O-3) ---

    #[tokio::test]
    async fn purge_old_csv_runs_deletes_completed_jobs_beyond_window() {
        let pool = test_pool().await;
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(91)).to_rfc3339();
        q!(
            "INSERT INTO csv_run_job \
             (id, org_id, name, request_json, status, total_rows, completed_rows, error_count, created_at) \
             VALUES ('job-old', '', 'old', '{}', 'completed', 1, 1, 0, ?)",
        )
        .bind(&old_ts)
        .execute(&pool)
        .await
        .unwrap();

        let deleted = purge_old_csv_runs(&pool, 90).await.unwrap();
        assert!(
            deleted >= 1,
            "expected at least 1 deleted row, got {deleted}"
        );
    }

    #[tokio::test]
    async fn purge_old_csv_runs_preserves_running_jobs() {
        let pool = test_pool().await;
        // An old 'running' job must never be purged (could be a long-running job or zombie).
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(365)).to_rfc3339();
        q!(
            "INSERT INTO csv_run_job \
             (id, org_id, name, request_json, status, total_rows, completed_rows, error_count, created_at) \
             VALUES ('job-running', '', 'run', '{}', 'running', 10, 5, 0, ?)",
        )
        .bind(&old_ts)
        .execute(&pool)
        .await
        .unwrap();

        // Pass 0 days (cutoff = now) so everything old would be eligible — but
        // 'running' must still be excluded by the WHERE status filter.
        let deleted = purge_old_csv_runs(&pool, 0).await.unwrap();
        assert_eq!(deleted, 0, "running jobs must not be purged");
    }

    // --- Phase 5 / Story 2: echo server integration tests ---
    // Tests bypass the HTTP creation endpoint via direct DB insert + execute_csv_run()
    // so the loopback SSRF guard (which protects the public API) doesn't interfere.
    // RADAR_TEST_ALLOW_LOCAL=1 is still needed for the per-row dispatch guard.

    async fn insert_job_and_run(
        pool: &sqlx::AnyPool,
        job_id: &str,
        url: &str,
        method: &str,
        capture_body: bool,
        rows: Vec<serde_json::Value>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let template = super::RequestTemplate {
            url: url.to_string(),
            method: method.to_string(),
            headers: vec![],
            body: String::new(),
            capture_body,
            enable_retry: false,
        };
        let request_json = serde_json::to_string(&template).unwrap();
        q!(
            "INSERT INTO csv_run_job \
             (id, org_id, name, request_json, status, total_rows, completed_rows, error_count, created_at) \
             VALUES (?, '', 'echo-test', ?, 'pending', ?, 0, 0, ?)",
        )
        .bind(job_id)
        .bind(&request_json)
        .bind(rows.len() as i64)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        super::execute_csv_run(pool.clone(), job_id.to_string(), template, rows).await;
    }

    async fn wait_for_status(pool: &sqlx::AnyPool, job_id: &str, timeout_ms: u64) -> String {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);
        loop {
            let s: String = qs!("SELECT status FROM csv_run_job WHERE id = ?")
                .bind(job_id)
                .fetch_one(pool)
                .await
                .unwrap();
            if s != "pending" && s != "running" {
                return s;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("job {job_id} did not finish within {timeout_ms}ms (status={s})");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn csv_run_with_echo_server_completes_successfully() {
        super::SSRF_BYPASS.with(|v| v.set(true));
        let echo = crate::test_helpers::spawn_echo_server().await;
        let url = format!("http://127.0.0.1:{}/echo", echo.addr.port());
        let pool = test_pool().await;
        let job_id = "job-echo-ok";

        insert_job_and_run(
            &pool,
            job_id,
            &url,
            "GET",
            false,
            vec![
                serde_json::json!({"id":"r1"}),
                serde_json::json!({"id":"r2"}),
            ],
        )
        .await;

        echo.wait_for_requests(2, 3000).await;
        assert_eq!(wait_for_status(&pool, job_id, 1000).await, "completed");

        let count: i64 =
            qs!("SELECT COUNT(*) FROM csv_run_result WHERE job_id = ? AND http_status = 200",)
                .bind(job_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 2, "both rows must record http_status=200");
        super::SSRF_BYPASS.with(|v| v.set(false));
    }

    #[tokio::test]
    async fn csv_run_capture_body_stores_response() {
        super::SSRF_BYPASS.with(|v| v.set(true));
        let echo = crate::test_helpers::spawn_echo_server().await;
        let url = format!("http://127.0.0.1:{}/echo", echo.addr.port());
        let pool = test_pool().await;
        let job_id = "job-echo-capture";

        insert_job_and_run(
            &pool,
            job_id,
            &url,
            "POST",
            true,
            vec![serde_json::json!({"id":"abc"})],
        )
        .await;

        echo.wait_for_requests(1, 3000).await;
        wait_for_status(&pool, job_id, 1000).await;

        let resp_body: Option<String> =
            qs!("SELECT response_body FROM csv_run_result WHERE job_id = ?")
                .bind(job_id)
                .fetch_optional(&pool)
                .await
                .unwrap()
                .flatten();
        assert!(
            resp_body.is_some(),
            "response_body must be captured when capture_body=true"
        );
        super::SSRF_BYPASS.with(|v| v.set(false));
    }

    #[tokio::test]
    async fn csv_run_completed_with_failures_on_server_error() {
        super::SSRF_BYPASS.with(|v| v.set(true));
        let echo = crate::test_helpers::spawn_echo_server().await;
        // Echo server returns 500 when ?status=500 is in the URL.
        let url = format!("http://127.0.0.1:{}/echo?status=500", echo.addr.port());
        let pool = test_pool().await;
        let job_id = "job-echo-5xx";

        insert_job_and_run(
            &pool,
            job_id,
            &url,
            "GET",
            false,
            vec![serde_json::json!({"id":"row1"})],
        )
        .await;

        // GET gets 3 retry attempts (0 s, 1 s, 4 s) — wait up to 8 s.
        assert_eq!(
            wait_for_status(&pool, job_id, 8000).await,
            "completed_with_failures",
            "all rows failed with 5xx → completed_with_failures"
        );
        super::SSRF_BYPASS.with(|v| v.set(false));
    }
}
