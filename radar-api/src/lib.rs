mod errors;
mod auth;
mod ai;
pub(crate) mod utils;
pub(crate) mod playground;
pub(crate) mod settings;
pub(crate) mod decisions;
pub(crate) mod acknowledgements;
pub(crate) mod catalog;
pub(crate) mod evolution;
pub(crate) mod services;
pub(crate) mod consumers;
pub(crate) mod sampling;
pub(crate) mod diffs;
pub(crate) mod ingestion;
pub(crate) mod ai_tests;
pub(crate) mod release_notes;
pub(crate) mod summary;
pub(crate) mod webhooks;
pub(crate) mod scans;
pub(crate) mod notifications;
pub(crate) mod csv_runner;
pub(crate) mod audit;
pub(crate) mod readiness;
pub(crate) mod scalar_update;

pub(crate) use errors::get_prometheus_handle;
pub(crate) use auth::{
    RequireAuth, JwtSecretExt,
    auth_middleware,
    oidc_login, oidc_callback, oidc_me, oidc_logout,
};
pub use settings::{purge_old_usage_events, expire_old_evidence};
pub use csv_runner::purge_old_csv_runs;
#[cfg(test)]
pub(crate) use chrono::{Duration, Utc};
#[cfg(test)]
pub(crate) use serde_json::Value;
#[cfg(test)]
pub(crate) use utils::{apply_evolution_rules, field_in_deny_list, is_severity_downgrade, normalise_path, parse_codeowners, path_matches};
#[cfg(test)]
pub(crate) use auth::{JwtClaims, sign_jwt};
#[cfg(test)]
pub(crate) use ai_tests::templates_from_changes;

use anyhow::Result;
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::any::AnyPoolOptions;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_http::{cors::{AllowOrigin, Any, CorsLayer}, services::ServeDir, timeout::TimeoutLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TD-4: Per-token (Bearer) / per-IP sliding-window rate limiter
// ---------------------------------------------------------------------------

const RATE_LIMIT_WINDOW_SECS: u64 = 60;

#[derive(Clone)]
struct RateLimiter {
    // key: Bearer token prefix (authenticated) or client IP (unauthenticated)
    // value: (request count, window start)
    state: Arc<Mutex<HashMap<String, (u32, std::time::Instant)>>>,
    max_per_minute: u32,
}

impl RateLimiter {
    fn new(max_per_minute: u32) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            max_per_minute,
        }
    }

    /// Remove entries whose window expired more than 2× the window ago.
    /// Called from a background task every 10 minutes to bound memory growth.
    fn prune_stale(&self) {
        let cutoff = std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS * 2);
        let now = std::time::Instant::now();
        self.state
            .lock()
            .unwrap()
            .retain(|_, (_, window_start)| now.duration_since(*window_start) < cutoff);
    }

    /// Returns `true` if the request is allowed; `false` if the limit is exceeded.
    fn check_and_record(&self, key: &str) -> bool {
        if self.max_per_minute == 0 {
            return true; // unlimited
        }
        let mut state = self.state.lock().unwrap();
        let now = std::time::Instant::now();
        let entry = state.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS) {
            *entry = (1, now);
            true
        } else if entry.0 < self.max_per_minute {
            entry.0 += 1;
            true
        } else {
            false
        }
    }
}

fn client_key(req: &Request, trust_proxy: bool) -> String {
    // Key on the client's NETWORK identity, never on the Authorization header.
    // Keying on an unvalidated Bearer token let an anonymous attacker send a
    // fresh random token per request to get a brand-new bucket (rate-limit
    // bypass) and grow the limiter map until pruned (memory exhaustion).
    //
    // X-Forwarded-For / X-Real-IP are client-supplied and spoofable, so they are
    // honoured only when RADAR_TRUST_PROXY asserts that a trusted reverse proxy
    // sets them. Otherwise the real socket peer address is used.
    if trust_proxy {
        if let Some(ip) = req
            .headers()
            .get("x-forwarded-for")
            .or_else(|| req.headers().get("x-real-ip"))
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
            .filter(|s| !s.is_empty())
        {
            return format!("ip:{ip}");
        }
    }
    // Real socket peer, populated by into_make_service_with_connect_info below.
    if let Some(ci) = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        return format!("ip:{}", ci.0.ip());
    }
    "unknown".to_string()
}



// ---------------------------------------------------------------------------
// Request-ID middleware
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RequestId(String);

async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(RequestId(id.clone()));

    let start = std::time::Instant::now();
    let mut res = next.run(req).await;

    let elapsed = start.elapsed().as_secs_f64();
    metrics::histogram!("request_duration_seconds").record(elapsed);

    if let Ok(val) = id.parse::<axum::http::HeaderValue>() {
        res.headers_mut().insert("x-request-id", val);
    }
    res
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Resolve a SQLite URL with a relative path to an absolute one and
/// pre-create the file so sqlx AnyPool can open it on all platforms.
/// Non-SQLite URLs and `sqlite::memory:` are returned unchanged.
pub fn resolve_db_url(db_url: &str) -> String {
    let Some(rest) = db_url.strip_prefix("sqlite:") else {
        return db_url.to_string();
    };
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() || rest == ":memory:" {
        return db_url.to_string();
    }
    let path = std::path::Path::new(rest);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if !abs.exists() {
        std::fs::File::create(&abs).ok();
    }
    // Forward slashes required for the sqlite: URL scheme on all platforms.
    let s = abs.to_string_lossy().replace('\\', "/");
    // Unix absolute paths start with '/'; Windows drive letters do not.
    if s.starts_with('/') {
        format!("sqlite://{s}")   // → sqlite:///unix/path
    } else {
        format!("sqlite:///{s}")  // → sqlite:///C:/win/path
    }
}

/// Derive the parent directory of the SQLite database file from a resolved
/// `sqlite://…` URL, so we can place override files alongside the database.
/// Returns `None` for in-memory databases and non-SQLite (Postgres) URLs.
fn derive_sqlite_parent(effective_url: &str) -> Option<std::path::PathBuf> {
    // Effective URLs produced by `resolve_db_url` look like:
    //   sqlite:///C:/Users/.../drift.db   (Windows)
    //   sqlite:///unix/abs/path/drift.db  (Unix — note triple slash)
    if !effective_url.starts_with("sqlite://") {
        return None;
    }
    // Replace the scheme so the standard `url` crate can parse the path.
    let as_file = effective_url.replacen("sqlite://", "file://", 1);
    let parsed = url::Url::parse(&as_file).ok()?;
    let file_path = parsed.to_file_path().ok()?;
    file_path.parent().map(|p| p.to_path_buf())
}

pub async fn run(
    db_url: &str,
    static_dir: Option<&str>,
    bind_addr: &str,
    rate_limit_per_minute: u32,
    max_body_bytes: usize,
) -> Result<()> {
    sqlx::any::install_default_drivers();

    let effective_url = resolve_db_url(db_url);
    let effective_url = effective_url.as_str();

    // Register override dir for the Scalar runtime-update feature.
    // `set` silently no-ops if called a second time (should not happen in production).
    scalar_update::OVERRIDE_DIR
        .set(derive_sqlite_parent(effective_url))
        .ok();

    let max_conns: u32 = std::env::var("RADAR_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let pool = AnyPoolOptions::new()
        .max_connections(max_conns)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .max_lifetime(std::time::Duration::from_secs(1800))
        .idle_timeout(std::time::Duration::from_secs(600))
        .connect(effective_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    info!("migrations applied");

    // Start background scheduler for scheduled spec scans (K-3)
    scans::start_scan_scheduler(pool.clone());

    // Start weekly email digest scheduler (K-5)
    notifications::start_digest_scheduler(pool.clone());

    // Re-dispatch webhook deliveries abandoned mid-flight on previous run (TD-K5)
    webhooks::start_webhook_outbox(pool.clone());

    let require_auth = std::env::var("RADAR_REQUIRE_AUTH")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let jwt_secret = std::env::var("RADAR_JWT_SECRET").ok().filter(|s| !s.is_empty());

    let limiter = Arc::new(RateLimiter::new(rate_limit_per_minute));

    // Prune stale rate-limit buckets every 10 minutes to bound memory growth.
    let limiter_for_prune = limiter.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            limiter_for_prune.prune_stale();
        }
    });

    let app = build_router(pool, static_dir, max_body_bytes, require_auth, jwt_secret);

    // X-Forwarded-For is honoured for rate-limit keying only behind a trusted
    // proxy; read once here rather than per-request.
    let trust_proxy = std::env::var("RADAR_TRUST_PROXY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // D-7: Add rate limiting as the outermost layer so it wraps the entire app.
    let app = app.layer(middleware::from_fn(move |req: Request, next: Next| {
        let lim = limiter.clone();
        async move {
            let key = client_key(&req, trust_proxy);
            if !lim.check_and_record(&key) {
                metrics::counter!("radar_rate_limit_rejections_total").increment(1);
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(json!({"error": "rate limit exceeded, please retry later"})),
                )
                    .into_response();
            }
            next.run(req).await
        }
    }));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!(
        bind = %listener.local_addr()?,
        rate_limit = rate_limit_per_minute,
        "radar-api listening"
    );

    // Attach the socket peer address so the rate limiter can key on the real
    // client IP rather than a spoofable header or unvalidated token.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}


// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router(pool: sqlx::AnyPool, static_dir: Option<&str>, max_body_bytes: usize, require_auth: bool, jwt_secret: Option<String>) -> Router {

    let v1 = Router::new()
        .route("/services", get(services::list_services).post(services::create_service))
        .route("/services/:id", get(services::get_service))
        .route("/services/:id/diffs", get(diffs::list_diffs).post(diffs::create_diff))
        .route("/services/:id/diffs/compare", post(diffs::compare_specs))
        .route("/compare/batch", post(diffs::batch_compare))
        .route("/services/:id/consumers", get(consumers::list_consumers))
        .route("/services/:id/subscriptions", post(consumers::create_subscription))
        .route("/consumers", get(consumers::list_all_consumers).post(consumers::create_consumer))
        .route("/consumers/upsert", post(consumers::upsert_consumer_by_name))
        .route("/evidence/collection", post(consumers::ingest_collection_evidence))
        .route("/diffs", get(diffs::list_all_diffs))
        .route("/diffs/:id", get(diffs::get_diff))
        .route("/diffs/:id/blast-radius", get(diffs::blast_radius))
        .route("/usage/events", post(ingestion::ingest_usage_event))
        .route("/otlp/v1/traces", post(ingestion::ingest_otlp_traces))
        .route("/gateway/logs", post(ingestion::ingest_gateway_logs))
        .route("/services/:id/sampling", get(sampling::get_sampling).put(sampling::put_sampling))
        .route("/evidence/coverage", get(sampling::evidence_coverage))
        .route("/call-sites", post(ingestion::upsert_call_sites))
        .route("/summary", get(summary::get_summary))
        .route("/generate-tests", post(ai_tests::generate_tests))
        .route("/generate-tests", get(ai_tests::list_test_suites))
        .route("/generate-tests/:id", get(ai_tests::get_test_suite))
        .route("/sandbox-envs", get(playground::list_sandbox_envs).post(playground::create_sandbox_env))
        .route("/sandbox-envs/:id", axum::routing::put(playground::update_sandbox_env).delete(playground::delete_sandbox_env))
        .route("/spec-versions", get(playground::list_spec_versions))
        .route("/spec-versions/:id/raw", get(playground::get_spec_version_raw))
        .route("/settings", get(settings::get_settings).put(settings::update_settings))
        .route("/settings/integrations", get(settings::get_integrations))
        .route("/release-notes", get(release_notes::list_release_notes))
        .route("/release-notes/:id", get(release_notes::get_release_note))
        .route("/release-notes/:id/status", axum::routing::patch(release_notes::patch_release_note_status))
        .route("/release-notes/:id/generate-status", get(release_notes::get_generate_status))
        .route("/diffs/:id/release-notes", post(release_notes::create_release_note))
        .route("/diffs/:id/release-notes/generate", post(release_notes::generate_release_note))
        .route("/diffs/:id/migration-guide", get(release_notes::get_migration_guide))
        .route("/diffs/:id/test-suites", get(ai_tests::list_diff_test_suites))
        .route("/policy-decisions", get(decisions::list_policy_decisions).post(decisions::create_policy_decision))
        .route("/acknowledgements", get(acknowledgements::list_acknowledgements).post(acknowledgements::create_acknowledgement))
        .route("/diffs/:id/acknowledgements", get(acknowledgements::list_diff_acknowledgements))
        .route("/catalog-sources", get(catalog::list_catalog_sources).post(catalog::create_catalog_source))
        .route("/catalog-sources/:id/sync", post(catalog::sync_catalog_source))
        .route("/evolution-rules", get(evolution::list_evolution_rules).post(evolution::create_evolution_rule))
        .route("/evolution-rules/:id", axum::routing::delete(evolution::delete_evolution_rule).patch(evolution::toggle_evolution_rule))
        .route("/webhooks", get(webhooks::list_webhooks).post(webhooks::create_webhook))
        .route("/webhooks/:id", axum::routing::delete(webhooks::delete_webhook))
        .route("/webhooks/:id/test", post(webhooks::test_webhook))
        .route("/webhooks/:id/deliveries", get(webhooks::list_deliveries))
        .route("/scheduled-scans", get(scans::list_scans).post(scans::create_scan))
        .route("/scheduled-scans/:id", axum::routing::delete(scans::delete_scan))
        .route("/scheduled-scans/history", get(scans::run_history))
        .route("/notifications/digest/preview", post(notifications::preview_digest))
        .route("/csv-runs", get(csv_runner::list_csv_runs).post(csv_runner::create_csv_run))
        .route("/csv-runs/:id", get(csv_runner::get_csv_run).delete(csv_runner::cancel_csv_run))
        .route("/csv-runs/:id/results", get(csv_runner::get_csv_run_results))
        .route("/audit-events", get(audit::list_audit_events).post(audit::create_audit_event))
        .route("/readiness", get(readiness::get_readiness))
        .layer(middleware::from_fn_with_state(pool.clone(), auth_middleware))
        // Outermost layer: inject RequireAuth + JwtSecretExt before auth_middleware runs.
        .layer(middleware::from_fn({
            let jwt_secret = jwt_secret.clone();
            move |mut req: Request, next: Next| {
                let s = jwt_secret.clone();
                async move {
                    req.extensions_mut().insert(RequireAuth(require_auth));
                    req.extensions_mut().insert(JwtSecretExt(s));
                    next.run(req).await
                }
            }
        }))
        .with_state(pool.clone());

    let cors = {
        let origins = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
        if origins.is_empty() {
            CorsLayer::permissive()
        } else {
            let list: Vec<axum::http::HeaderValue> = origins
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if list.is_empty() {
                CorsLayer::permissive()
            } else {
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(list))
                    .allow_methods(Any)
                    .allow_headers(Any)
            }
        }
    };

    // Ensure the recorder is installed before any request arrives.
    get_prometheus_handle();

    let timeout_secs = std::env::var("RADAR_REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics_handler))
        .route("/scalar.js", get(serve_scalar_js))
        .route("/scalar/version", get(scalar_update::get_version))
        .route("/scalar/update", post(scalar_update::post_update))
        .route("/auth/login", get(oidc_login))
        .route("/auth/callback", get(oidc_callback))
        .route("/auth/me", get(oidc_me))
        .route("/auth/logout", get(oidc_logout))
        .route("/share/:token", get(diffs::get_shared_diff))
        .nest("/v1", v1)
        .with_state(pool.clone())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(timeout_secs)))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &Request| {
                let id = req
                    .extensions()
                    .get::<RequestId>()
                    .map(|r| r.0.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                tracing::info_span!(
                    "request",
                    method = %req.method(),
                    uri = %req.uri(),
                    request_id = %id,
                )
            }),
        )
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn(move |mut req: Request, next: Next| {
            let s = jwt_secret.clone();
            async move {
                req.extensions_mut().insert(JwtSecretExt(s));
                next.run(req).await
            }
        }))
        .layer(cors);

    if let Some(dir) = static_dir {
        // SPA fallback: serve index.html for any path not found in the static directory
        // so that React Router's client-side routes work on hard refresh.
        let index = format!("{dir}/index.html");
        app = app.nest_service(
            "/app",
            ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)),
        );
    }

    app
}

// ---------------------------------------------------------------------------
// Request body types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct PaginationParams {
    #[serde(default = "default_page_limit")]
    pub(crate) limit: i64,
    #[serde(default)]
    pub(crate) offset: i64,
}

fn default_page_limit() -> i64 {
    50
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Serve the Scalar API-reference standalone browser bundle.
///
/// Prefers a disk-based override (written by `POST /scalar/update`) over the
/// compiled-in bundle.  The compiled-in bundle is cached for 24 h as immutable;
/// an override is cached for 1 h so a subsequent update is picked up quickly.
async fn serve_scalar_js() -> impl IntoResponse {
    let (bytes, is_bundled) = scalar_update::active_js();
    let cache_control = if is_bundled {
        "public, max-age=86400, immutable"
    } else {
        "public, max-age=3600"
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, cache_control),
        ],
        bytes,
    )
}

async fn health(State(pool): State<sqlx::AnyPool>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&pool).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({"status": "ok", "db": "ok", "version": "0.1.0"})),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "degraded", "db": "unreachable", "version": "0.1.0"})),
        ),
    }
}

// GET /metrics — Prometheus text exposition
async fn metrics_handler(headers: axum::http::HeaderMap) -> impl IntoResponse {
    // When RADAR_METRICS_TOKEN is set, require a matching Bearer token.
    // If unset, the endpoint is open (backwards-compatible for desktop / CI).
    if let Ok(expected) = std::env::var("RADAR_METRICS_TOKEN") {
        let provided = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let ok = provided
            .map(|p| crate::utils::constant_time_eq(p.as_bytes(), expected.as_bytes()))
            .unwrap_or(false);
        if !ok {
            return (StatusCode::UNAUTHORIZED, "").into_response();
        }
    }
    let body = get_prometheus_handle().render();
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test_helpers;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::Request as HttpRequest,
    };
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    async fn test_pool() -> sqlx::AnyPool {
        sqlx::any::install_default_drivers();
        // When DATABASE_URL points at Postgres (set in the rust-postgres CI job), run
        // the full test suite against a real Postgres instance to catch SQL dialect gaps.
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite::memory:".to_string());
        let is_sqlite = url.starts_with("sqlite");

        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("failed to create test pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to run migrations");
        if is_sqlite {
            // Enable FK enforcement to match PostgreSQL behaviour.
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn test_ingest_usage_events_accepted() {
        let pool = test_pool().await;

        // Insert prerequisite rows to satisfy FK constraints.
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("consumer-a")
        .bind("Consumer A")
        .bind("https://github.com/acme/a")
        .bind("team-a")
        .bind("a@acme.com")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("service-b")
        .bind("Service B")
        .bind("https://github.com/acme/b")
        .bind("team-b")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!([
            {
                "consumer_id": "consumer-a",
                "service_id": "service-b",
                "operation": "GET /users"
            },
            {
                "consumer_id": "consumer-a",
                "service_id": "service-b",
                "operation": "POST /users"
            }
        ]);

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/usage/events")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["accepted"], 2);
    }

    #[tokio::test]
    async fn test_ingest_too_large_batch_rejected() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        // Build a batch of 501 events.
        let events: Vec<serde_json::Value> = (0..501)
            .map(|i| {
                serde_json::json!({
                    "consumer_id": format!("consumer-{i}"),
                    "service_id": format!("service-{i}"),
                    "operation": "GET /ping"
                })
            })
            .collect();

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/usage/events")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&events).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"], "batch too large, max 500");
    }

    #[tokio::test]
    async fn test_purge_old_events() {
        let pool = test_pool().await;

        // Insert prerequisite consumer and service rows to satisfy FK constraints.
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("consumer-old")
        .bind("Old Consumer")
        .bind("https://github.com/acme/old")
        .bind("team-old")
        .bind("old@acme.com")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("consumer-new")
        .bind("New Consumer")
        .bind("https://github.com/acme/new")
        .bind("team-new")
        .bind("new@acme.com")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("service-old")
        .bind("Old Service")
        .bind("https://github.com/acme/svc-old")
        .bind("team-old")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("service-new")
        .bind("New Service")
        .bind("https://github.com/acme/svc-new")
        .bind("team-new")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        // Insert a usage_event with recorded_at 100 days ago.
        let old_id = Uuid::new_v4().to_string();
        let old_ts = (Utc::now() - Duration::days(100)).to_rfc3339();
        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, recorded_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&old_id)
        .bind("consumer-old")
        .bind("service-old")
        .bind("GET /old")
        .bind(&old_ts)
        .execute(&pool)
        .await
        .unwrap();

        // Purge rows older than 30 days — should delete the 100-day-old row.
        let deleted = purge_old_usage_events(&pool, 30).await.unwrap();
        assert!(deleted >= 1, "expected at least 1 deleted row, got {deleted}");

        // Insert a fresh event (recorded_at = now).
        let fresh_id = Uuid::new_v4().to_string();
        let fresh_ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, recorded_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&fresh_id)
        .bind("consumer-new")
        .bind("service-new")
        .bind("GET /new")
        .bind(&fresh_ts)
        .execute(&pool)
        .await
        .unwrap();

        // Purge again — fresh event should NOT be deleted.
        let deleted2 = purge_old_usage_events(&pool, 30).await.unwrap();
        assert_eq!(deleted2, 0, "fresh event should not be purged, but got {deleted2} deletions");
    }

    // -----------------------------------------------------------------------
    // blast_radius tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_blast_radius_404_for_unknown_diff() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/diffs/00000000-0000-0000-0000-000000000000/blast-radius")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["error"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_blast_radius_empty_for_no_consumers() {
        let pool = test_pool().await;

        // Create a service.
        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id)
        .bind("test-svc")
        .bind("https://github.com/acme/test")
        .bind("platform")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        // Create spec versions.
        let from_sv_id = Uuid::new_v4().to_string();
        let to_sv_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&from_sv_id)
        .bind(&service_id)
        .bind("v1.0")
        .bind(&now)
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&to_sv_id)
        .bind(&service_id)
        .bind("v1.1")
        .bind(&now)
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        // Create diff.
        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id)
        .bind(&from_sv_id)
        .bind(&to_sv_id)
        .bind::<Option<String>>(None)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // Create change.
        let change_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&change_id)
        .bind(&diff_id)
        .bind("GET /users")
        .bind("operation_removed")
        .bind("breaking")
        .bind::<Option<String>>(None)
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["diff_id"], diff_id);
        let entries = json["entries"].as_array().unwrap();
        assert!(entries.is_empty(), "expected empty entries, got: {:?}", entries);
    }

    #[tokio::test]
    async fn test_blast_radius_includes_consumer_with_usage() {
        let pool = test_pool().await;

        // Create a service.
        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id)
        .bind("payments-api")
        .bind("https://github.com/acme/payments")
        .bind("platform")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        // Create consumer.
        let consumer_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&consumer_id)
        .bind("billing-svc")
        .bind("https://github.com/acme/billing")
        .bind("billing-team")
        .bind("billing@acme.com")
        .execute(&pool)
        .await
        .unwrap();

        // Subscribe consumer to service.
        let sub_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&sub_id)
        .bind(&service_id)
        .bind(&consumer_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // Create spec versions.
        let from_sv_id = Uuid::new_v4().to_string();
        let to_sv_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&from_sv_id)
        .bind(&service_id)
        .bind("v1.0")
        .bind(&now)
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&to_sv_id)
        .bind(&service_id)
        .bind("v1.1")
        .bind(&now)
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        // Create diff.
        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id)
        .bind(&from_sv_id)
        .bind(&to_sv_id)
        .bind::<Option<String>>(None)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // Create change for GET /users → response.phone.
        let change_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&change_id)
        .bind(&diff_id)
        .bind("GET /users \u{2192} response.phone")
        .bind("field_removed")
        .bind("breaking")
        .bind::<Option<String>>(None)
        .execute(&pool)
        .await
        .unwrap();

        // Insert a usage_event from billing-svc for GET /users (within last 7 days → High).
        let event_id = Uuid::new_v4().to_string();
        // Use current time (well within 7 days).
        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&event_id)
        .bind(&consumer_id)
        .bind(&service_id)
        .bind("GET /users")
        .bind("phone")
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["diff_id"], diff_id);
        assert_eq!(json["service_id"], service_id);

        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "expected 1 entry, got: {:?}", entries);

        let entry = &entries[0];
        assert_eq!(entry["consumer"]["name"], "billing-svc");
        assert_eq!(entry["confidence"], "high");
        assert_eq!(entry["has_runtime_usage"], true);
    }

    #[tokio::test]
    async fn test_blast_radius_call_site_only_is_low_confidence() {
        let pool = test_pool().await;

        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id)
        .bind("svc-cs")
        .bind("https://github.com/acme/svc-cs")
        .bind("team-cs")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        let consumer_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&consumer_id)
        .bind("invoicing-svc")
        .bind("https://github.com/acme/invoicing")
        .bind("billing-team")
        .bind("inv@acme.com")
        .execute(&pool)
        .await
        .unwrap();

        let now = Utc::now().to_rfc3339();
        let sub_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&sub_id)
        .bind(&service_id)
        .bind(&consumer_id)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let from_sv_id = Uuid::new_v4().to_string();
        let to_sv_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&from_sv_id).bind(&service_id).bind("v1.0").bind(&now).bind("openapi")
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&to_sv_id).bind(&service_id).bind("v1.1").bind(&now).bind("openapi")
        .execute(&pool).await.unwrap();

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id).bind(&from_sv_id).bind(&to_sv_id).bind::<Option<String>>(None).bind(&now)
        .execute(&pool).await.unwrap();

        let change_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&change_id).bind(&diff_id).bind("GET /invoices").bind("operation_removed").bind("breaking").bind::<Option<String>>(None)
        .execute(&pool).await.unwrap();

        // Only a call_site — no usage_event.
        let cs_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO call_site (id, consumer_id, service_id, operation, file_path, line_number, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&cs_id).bind(&consumer_id).bind(&service_id)
        .bind("GET /invoices").bind("src/api.rs").bind(42i64).bind(&now)
        .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["confidence"], "low");
        assert_eq!(entries[0]["has_runtime_usage"], false);
        assert_eq!(entries[0]["has_call_site"], true);
    }

    #[tokio::test]
    async fn test_create_consumer_returns_201() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "name": "billing-svc",
            "repo_url": "https://github.com/acme/billing",
            "owner_team": "platform",
            "contact": "billing@acme.com"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["name"], "billing-svc");
        assert!(json["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_create_consumer_validates_name() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "name": "",
            "repo_url": "https://github.com/acme/billing",
            "owner_team": "platform",
            "contact": "billing@acme.com"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_subscribe_consumer_to_service() {
        let pool = test_pool().await;

        // Insert a service directly.
        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id)
        .bind("payments-api")
        .bind("https://github.com/acme/payments")
        .bind("platform")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        // 1. Create consumer.
        let consumer_body = serde_json::json!({
            "name": "billing-svc",
            "repo_url": "https://github.com/acme/billing",
            "owner_team": "platform",
            "contact": "billing@acme.com"
        });
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&consumer_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let consumer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let consumer_id = consumer["id"].as_str().unwrap().to_string();

        // 2. Subscribe consumer to service — expect 201.
        let sub_body = serde_json::json!({ "consumer_id": consumer_id });
        let req = HttpRequest::builder()
            .method("POST")
            .uri(format!("/v1/services/{service_id}/subscriptions"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&sub_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let sub: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(sub["service_id"], service_id);
        assert_eq!(sub["consumer_id"], consumer_id);
        assert!(sub["id"].as_str().is_some());

        // 3. Subscribe again — idempotent, expect 200.
        let req = HttpRequest::builder()
            .method("POST")
            .uri(format!("/v1/services/{service_id}/subscriptions"))
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({ "consumer_id": consumer_id })).unwrap(),
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_all_diffs_returns_diff_with_counts() {
        let pool = test_pool().await;
        let now = Utc::now().to_rfc3339();

        let service_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind(&service_id).bind("list-api").bind("https://github.com/acme/list").bind("platform").bind("openapi")
            .execute(&pool).await.unwrap();

        let from_sv = Uuid::new_v4().to_string();
        let to_sv = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind(&from_sv).bind(&service_id).bind("main").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind(&to_sv).bind(&service_id).bind("pr-1").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&diff_id).bind(&from_sv).bind(&to_sv).bind::<Option<String>>(None).bind(&now)
            .execute(&pool).await.unwrap();

        sqlx::query("INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(Uuid::new_v4().to_string()).bind(&diff_id).bind("GET /items").bind("operation_removed").bind("breaking").bind::<Option<String>>(None)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(Uuid::new_v4().to_string()).bind(&diff_id).bind("GET /items → response.name").bind("field_added").bind("safe").bind::<Option<String>>(None)
            .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET").uri("/v1/diffs").body(Body::empty()).unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = json.as_array().unwrap();
        // Find our specific diff in the response — don't assert arr.len() == 1 since a
        // shared Postgres DB (used in the rust-postgres CI job) may contain diffs from
        // other parallel tests.
        let our_diff = arr.iter().find(|e| e["id"] == diff_id)
            .expect("diff should appear in list_all_diffs response");
        assert_eq!(our_diff["service_name"], "list-api");
        assert_eq!(our_diff["breaking_count"], 1);
        assert_eq!(our_diff["safe_count"], 1);
    }

    #[tokio::test]
    async fn test_summary_breaking_changes_and_services_count() {
        let pool = test_pool().await;
        let now = Utc::now().to_rfc3339();

        let service_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind(&service_id).bind("summary-api").bind("https://github.com/acme/summary").bind("platform").bind("openapi")
            .execute(&pool).await.unwrap();

        let from_sv = Uuid::new_v4().to_string();
        let to_sv = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind(&from_sv).bind(&service_id).bind("v1").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind(&to_sv).bind(&service_id).bind("v2").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&diff_id).bind(&from_sv).bind(&to_sv).bind::<Option<String>>(None).bind(&now)
            .execute(&pool).await.unwrap();

        for path in &["GET /users", "POST /orders"] {
            sqlx::query("INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(Uuid::new_v4().to_string()).bind(&diff_id).bind(path).bind("operation_removed").bind("breaking").bind::<Option<String>>(None)
                .execute(&pool).await.unwrap();
        }

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET").uri("/v1/summary").body(Body::empty()).unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Use >= so the test stays green on a shared Postgres DB where other parallel
        // tests may have already inserted services or breaking changes.
        assert!(json["breaking_changes_30d"].as_i64().unwrap_or(0) >= 2);
        assert!(json["services_count"].as_i64().unwrap_or(0) >= 1);
    }

    #[tokio::test]
    async fn test_create_service_returns_201() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "name": "payments-api",
            "repo_url": "https://github.com/acme/payments",
            "owner_team": "payments",
            "spec_format": "openapi"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/services")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["name"], "payments-api");
        assert!(json["id"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_create_service_with_explicit_id_then_get() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let id = Uuid::new_v4().to_string();
        let body = serde_json::json!({
            "id": id,
            "name": "orders-api",
            "repo_url": "https://github.com/acme/orders",
            "owner_team": "commerce",
            "spec_format": "openapi"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/services")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Fetch by ID.
        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/services/{id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["id"], id);
        assert_eq!(json["name"], "orders-api");
    }

    #[tokio::test]
    async fn test_get_service_404_for_unknown() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/services/does-not-exist")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_diffs_pagination_params_accepted() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/diffs?limit=10&offset=0")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.is_array());
    }

    #[tokio::test]
    async fn test_diff_deduplication_returns_cached() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let svc_id = Uuid::new_v4().to_string();
        let diff_body = serde_json::json!({
            "service_name": "dedup-svc",
            "repo_url": "",
            "owner_team": "",
            "from_git_ref": "aaa",
            "to_git_ref": "bbb",
            "spec_format": "openapi",
            "changes": []
        });

        // First submission — creates the diff (201).
        let req = HttpRequest::builder()
            .method("POST")
            .uri(format!("/v1/services/{svc_id}/diffs"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&diff_body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let first: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Second submission — same transition returns cached (200).
        let req = HttpRequest::builder()
            .method("POST")
            .uri(format!("/v1/services/{svc_id}/diffs"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&diff_body).unwrap()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let second: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Same diff ID returned.
        assert_eq!(first["id"], second["id"]);
        assert_eq!(second["cached"], true);
    }

    #[tokio::test]
    async fn test_health_returns_ok_with_live_db() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "ok");
        assert_eq!(body["db"], "ok");
    }

    #[tokio::test]
    async fn test_health_returns_degraded_when_pool_closed() {
        let pool = test_pool().await;
        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);
        pool.close().await;

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["db"], "unreachable");
    }

    #[tokio::test]
    async fn test_require_auth_blocks_unauthenticated_requests() {
        let pool = test_pool().await;
        // Pass require_auth=true directly — no env var manipulation needed.
        let app = build_router(pool, None, 4 * 1024 * 1024, true, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/services")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_sandbox_env_token_masked_in_response() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/sandbox-envs")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"my-env","base_url":"https://api.example.com","bearer_token":"supersecrettoken123"}"#,
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let masked = json["bearer_token"].as_str().unwrap();
        assert!(masked.starts_with("***"), "expected masked token, got: {masked}");
        assert!(masked.ends_with("n123"), "expected last-4 suffix, got: {masked}");
        assert_eq!(json["bearer_token_set"], true);
    }

    #[tokio::test]
    async fn test_list_sandbox_envs_token_masked() {
        let pool = test_pool().await;
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO sandbox_env (id, name, base_url, bearer_token, description, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind("prod-env")
        .bind("https://prod.example.com")
        .bind("very-long-secret-token")
        .bind("")
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/sandbox-envs")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let items = json.as_array().unwrap();
        assert_eq!(items.len(), 1);
        let masked = items[0]["bearer_token"].as_str().unwrap();
        assert!(masked.starts_with("***"), "expected masked token, got: {masked}");
        assert!(masked.ends_with("oken"), "expected last-4 suffix, got: {masked}");
        assert_eq!(items[0]["bearer_token_set"], true);
    }

    #[tokio::test]
    async fn test_create_service_writes_org_id() {
        let pool = test_pool().await;
        let claims = JwtClaims { sub: "u1".into(), org_id: "acme-corp".into(), exp: usize::MAX };
        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None).layer(
            axum::middleware::from_fn(
                move |mut req: axum::extract::Request, next: axum::middleware::Next| {
                    let c = claims.clone();
                    async move {
                        req.extensions_mut().insert(c);
                        next.run(req).await
                    }
                },
            ),
        );

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/services")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"svc-org-test","repo_url":"","owner_team":"team","spec_format":"openapi"}"#,
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let row: (String,) =
            sqlx::query_as("SELECT org_id FROM service WHERE name = 'svc-org-test'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(row.0, "acme-corp");
    }

    #[tokio::test]
    async fn test_list_services_filtered_by_org_id() {
        let pool = test_pool().await;

        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("svc-alpha").bind("alpha-api").bind("").bind("team-a").bind("openapi").bind("org-alpha")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("svc-beta").bind("beta-api").bind("").bind("team-b").bind("openapi").bind("org-beta")
        .execute(&pool).await.unwrap();

        let claims = JwtClaims { sub: "u1".into(), org_id: "org-alpha".into(), exp: usize::MAX };
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None).layer(
            axum::middleware::from_fn(
                move |mut req: axum::extract::Request, next: axum::middleware::Next| {
                    let c = claims.clone();
                    async move {
                        req.extensions_mut().insert(c);
                        next.run(req).await
                    }
                },
            ),
        );

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/services")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let items = json.as_array().unwrap();
        assert_eq!(items.len(), 1, "expected only org-alpha service");
        assert_eq!(items[0]["name"], "alpha-api");
    }

    #[tokio::test]
    async fn test_blast_radius_entry_has_evidence() {
        let pool = test_pool().await;
        let now = Utc::now().to_rfc3339();

        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id).bind("evidence-svc").bind("").bind("team").bind("openapi")
        .execute(&pool).await.unwrap();

        let consumer_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&consumer_id).bind("evidence-consumer").bind("").bind("team").bind("e@t.com")
        .execute(&pool).await.unwrap();

        let sub_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&sub_id).bind(&service_id).bind(&consumer_id).bind(&now)
        .execute(&pool).await.unwrap();

        let from_sv_id = Uuid::new_v4().to_string();
        let to_sv_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&from_sv_id).bind(&service_id).bind("v1.0").bind(&now).bind("openapi")
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&to_sv_id).bind(&service_id).bind("v1.1").bind(&now).bind("openapi")
        .execute(&pool).await.unwrap();

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id).bind(&from_sv_id).bind(&to_sv_id).bind::<Option<String>>(None).bind(&now)
        .execute(&pool).await.unwrap();

        let change_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&change_id).bind(&diff_id)
        .bind("GET /items \u{2192} response.id")
        .bind("field_removed").bind("breaking").bind::<Option<String>>(None)
        .execute(&pool).await.unwrap();

        let event_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&event_id).bind(&consumer_id).bind(&service_id)
        .bind("GET /items").bind("id").bind(&now)
        .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);

        let evidence = entries[0]["evidence"]
            .as_array()
            .expect("evidence must be an array");
        assert!(!evidence.is_empty(), "evidence array must not be empty");
        assert_eq!(evidence[0]["kind"], "runtime_usage");
        assert_eq!(evidence[0]["operation"], "GET /items");
    }

    // ── E-1: impact_evidence persistence tests ───────────────────────────────

    #[tokio::test]
    async fn test_blast_radius_writes_to_impact_evidence() {
        let pool = test_pool().await;
        let now = Utc::now().to_rfc3339();

        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id).bind("ev-write-svc").bind("").bind("team").bind("openapi")
        .execute(&pool).await.unwrap();

        let consumer_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&consumer_id).bind("ev-write-consumer").bind("").bind("team").bind("e@t.com")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&service_id).bind(&consumer_id).bind(&now)
        .execute(&pool).await.unwrap();

        let from_sv = Uuid::new_v4().to_string();
        let to_sv = Uuid::new_v4().to_string();
        for (id, git_ref) in [(&from_sv, "v1"), (&to_sv, "v2")] {
            sqlx::query(
                "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id).bind(&service_id).bind(git_ref).bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        }

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id).bind(&from_sv).bind(&to_sv).bind::<Option<String>>(None).bind(&now)
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&diff_id)
        .bind("GET /widgets \u{2192} response.price")
        .bind("field_removed").bind("breaking").bind::<Option<String>>(None)
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&consumer_id).bind(&service_id)
        .bind("GET /widgets").bind("price").bind(&now)
        .execute(&pool).await.unwrap();

        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify the blast-radius response has an entry.
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["entries"].as_array().unwrap().len(), 1);

        // Core assertion: impact_evidence must have at least one row for this diff.
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM impact_evidence WHERE diff_id = ?",
        )
        .bind(&diff_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count > 0, "impact_evidence must have rows after blast_radius call");
    }

    // M-17-T1: a GET must be idempotent — repeat calls for unchanged evidence must
    // NOT append new impact_evidence rows (deterministic id + ON CONFLICT DO NOTHING).
    #[tokio::test]
    async fn test_blast_radius_evidence_write_is_idempotent() {
        let pool = test_pool().await;
        let now = Utc::now().to_rfc3339();

        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id).bind("idem-svc").bind("").bind("team").bind("openapi")
        .execute(&pool).await.unwrap();

        let consumer_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&consumer_id).bind("idem-consumer").bind("").bind("team").bind("e@t.com")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&service_id).bind(&consumer_id).bind(&now)
        .execute(&pool).await.unwrap();

        let from_sv = Uuid::new_v4().to_string();
        let to_sv = Uuid::new_v4().to_string();
        for (id, git_ref) in [(&from_sv, "v1"), (&to_sv, "v2")] {
            sqlx::query(
                "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id).bind(&service_id).bind(git_ref).bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        }

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id).bind(&from_sv).bind(&to_sv).bind::<Option<String>>(None).bind(&now)
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&diff_id)
        .bind("GET /widgets \u{2192} response.price")
        .bind("field_removed").bind("breaking").bind::<Option<String>>(None)
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&consumer_id).bind(&service_id)
        .bind("GET /widgets").bind("price").bind(&now)
        .execute(&pool).await.unwrap();

        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);

        let call = |app: axum::Router, diff_id: String| async move {
            let req = HttpRequest::builder()
                .method("GET")
                .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        };

        // First GET seeds evidence.
        call(app.clone(), diff_id.clone()).await;
        let count_after_first: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM impact_evidence WHERE diff_id = ?",
        )
        .bind(&diff_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count_after_first > 0, "first GET must write evidence");

        // Second GET must NOT grow the append-only table.
        call(app.clone(), diff_id.clone()).await;
        let count_after_second: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM impact_evidence WHERE diff_id = ?",
        )
        .bind(&diff_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count_after_first, count_after_second,
            "repeat blast-radius GET must be idempotent (no new impact_evidence rows)"
        );
    }

    #[tokio::test]
    async fn test_blast_radius_max_age_days_excludes_old_evidence() {
        let pool = test_pool().await;
        // Record usage 10 days ago — older than max_age_days=7 → consumer excluded.
        let old_ts = (Utc::now() - Duration::days(10)).to_rfc3339();
        let now = Utc::now().to_rfc3339();

        let service_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&service_id).bind("max-age-svc").bind("").bind("team").bind("openapi")
        .execute(&pool).await.unwrap();

        let consumer_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&consumer_id).bind("old-consumer").bind("").bind("team").bind("e@t.com")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&service_id).bind(&consumer_id).bind(&now)
        .execute(&pool).await.unwrap();

        let from_sv = Uuid::new_v4().to_string();
        let to_sv = Uuid::new_v4().to_string();
        for (id, git_ref) in [(&from_sv, "v1"), (&to_sv, "v2")] {
            sqlx::query(
                "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id).bind(&service_id).bind(git_ref).bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        }

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id).bind(&from_sv).bind(&to_sv).bind::<Option<String>>(None).bind(&now)
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&diff_id)
        .bind("GET /orders \u{2192} response.total")
        .bind("field_removed").bind("breaking").bind::<Option<String>>(None)
        .execute(&pool).await.unwrap();

        // Usage event recorded 10 days ago — outside a 7-day window.
        sqlx::query(
            "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&consumer_id).bind(&service_id)
        .bind("GET /orders").bind("total").bind(&old_ts)
        .execute(&pool).await.unwrap();

        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);

        // Without max_age_days: consumer should appear (30-day lookback includes 10 days ago).
        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json["entries"].as_array().unwrap().len(),
            1,
            "consumer should appear without max_age_days filter"
        );

        // With max_age_days=7: evidence is 10 days old → consumer excluded.
        let req2 = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius?max_age_days=7"))
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        let bytes2 = resp2.into_body().collect().await.unwrap().to_bytes();
        let json2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        assert_eq!(
            json2["entries"].as_array().unwrap().len(),
            0,
            "consumer with 10-day-old evidence must be excluded by max_age_days=7"
        );
    }

    // ── D-4: OIDC /auth/me tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_auth_me_returns_503_when_oidc_not_configured() {
        let pool = test_pool().await;
        // No jwt_secret → OIDC not configured → 503
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/auth/me")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 503);
    }

    // ── E-2: Cross-org 403 isolation matrix ─────────────────────────────────

    const E2_SECRET: &str = "e2-org-isolation-secret-42";

    fn make_org_jwt(org_id: &str) -> String {
        sign_jwt(
            &JwtClaims {
                sub: "test-user".into(),
                org_id: org_id.into(),
                exp: 9_999_999_999,
            },
            E2_SECRET,
        )
        .expect("sign_jwt must succeed in tests")
    }

    /// Insert a service owned by "org-beta" with a diff and spec version.
    /// Returns (service_id, from_sv_id, to_sv_id, diff_id).
    async fn setup_beta_service(pool: &sqlx::AnyPool) -> (String, String, String, String) {
        let service_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&service_id).bind("beta-svc").bind("https://github.com/beta/svc")
        .bind("beta-team").bind("openapi").bind("org-beta")
        .execute(pool).await.unwrap();

        let from_sv = Uuid::new_v4().to_string();
        let to_sv = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&from_sv).bind(&service_id).bind("v1.0").bind(&now).bind("openapi")
        .execute(pool).await.unwrap();

        // to_sv has spec_yaml so raw endpoint can confirm org check fires before content check.
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&to_sv).bind(&service_id).bind("v1.1").bind(&now).bind("openapi").bind("openapi: 3.0.0\n")
        .execute(pool).await.unwrap();

        let diff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&diff_id).bind(&from_sv).bind(&to_sv).bind::<Option<String>>(None).bind(&now)
        .execute(pool).await.unwrap();

        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&diff_id)
        .bind("GET /items").bind("operation_removed").bind("breaking").bind::<Option<String>>(None)
        .execute(pool).await.unwrap();

        (service_id, from_sv, to_sv, diff_id)
    }

    #[tokio::test]
    async fn test_e2_get_diff_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/diffs/:id must return 403");
    }

    #[tokio::test]
    async fn test_e2_get_service_cross_org_returns_403() {
        let pool = test_pool().await;
        let (service_id, _, _, _) = setup_beta_service(&pool).await;
        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/services/{service_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/services/:id must return 403");
    }

    #[tokio::test]
    async fn test_e2_blast_radius_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/diffs/:id/blast-radius must return 403");
    }

    #[tokio::test]
    async fn test_e2_get_spec_version_raw_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, to_sv, _) = setup_beta_service(&pool).await;
        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/spec-versions/{to_sv}/raw"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/spec-versions/:id/raw must return 403");
    }

    #[tokio::test]
    async fn test_e2_get_test_suite_cross_org_returns_403() {
        let pool = test_pool().await;
        let (service_id, _, _, _) = setup_beta_service(&pool).await;
        let suite_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO generated_test_suite (id, service_id, collection_name, collection_json, test_count, happy_count, negative_count, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&suite_id).bind(&service_id).bind("beta-suite").bind("{}")
        .bind(0i64).bind(0i64).bind(0i64).bind(&now)
        .execute(&pool).await.unwrap();

        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/generate-tests/{suite_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/generate-tests/:id must return 403");
    }

    #[tokio::test]
    async fn test_e2_list_service_diffs_cross_org_returns_403() {
        let pool = test_pool().await;
        let (service_id, _, _, _) = setup_beta_service(&pool).await;
        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/services/{service_id}/diffs"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/services/:id/diffs must return 403");
    }

    #[tokio::test]
    async fn test_e2_list_consumers_cross_org_returns_403() {
        let pool = test_pool().await;
        let (service_id, _, _, _) = setup_beta_service(&pool).await;
        let token = make_org_jwt("org-alpha");
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()));

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/services/{service_id}/consumers"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/services/:id/consumers must return 403");
    }

    // ── M-8: Org isolation sweep — cross-org 403 matrix for the handlers that
    //         previously performed no org check. Each resource is owned by
    //         "org-beta"; the caller presents an "org-alpha" JWT and must get 403.

    /// Insert a release_note for the beta diff. Returns the note id.
    async fn insert_beta_release_note(pool: &sqlx::AnyPool, diff_id: &str) -> String {
        let note_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO release_note (id, diff_id, content, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&note_id).bind(diff_id).bind("beta content").bind(&now)
        .execute(pool).await.unwrap();
        note_id
    }

    /// Insert a consumer owned by org-beta. Returns the consumer id.
    async fn insert_beta_consumer(pool: &sqlx::AnyPool) -> String {
        let cid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact, org_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&cid).bind("beta-consumer").bind("").bind("").bind("").bind("org-beta")
        .execute(pool).await.unwrap();
        cid
    }

    fn alpha_app(pool: sqlx::AnyPool) -> Router {
        build_router(pool, None, 4 * 1024 * 1024, false, Some(E2_SECRET.to_string()))
    }

    fn alpha_req(method: &str, uri: String, body: Option<serde_json::Value>) -> HttpRequest<Body> {
        let token = make_org_jwt("org-alpha");
        let mut b = HttpRequest::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"));
        match body {
            Some(v) => {
                b = b.header("content-type", "application/json");
                b.body(Body::from(v.to_string())).unwrap()
            }
            None => b.body(Body::empty()).unwrap(),
        }
    }

    #[tokio::test]
    async fn test_m8_get_release_note_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let note_id = insert_beta_release_note(&pool, &diff_id).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("GET", format!("/v1/release-notes/{note_id}"), None))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET /v1/release-notes/:id must return 403");
    }

    #[tokio::test]
    async fn test_m8_patch_release_note_status_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let note_id = insert_beta_release_note(&pool, &diff_id).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("PATCH", format!("/v1/release-notes/{note_id}/status"),
                Some(serde_json::json!({"status": "reviewed"}))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org PATCH release-note status must return 403");
    }

    #[tokio::test]
    async fn test_m8_generate_status_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let note_id = insert_beta_release_note(&pool, &diff_id).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("GET", format!("/v1/release-notes/{note_id}/generate-status"), None))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org GET generate-status must return 403");
    }

    #[tokio::test]
    async fn test_m8_create_release_note_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("POST", format!("/v1/diffs/{diff_id}/release-notes"),
                Some(serde_json::json!({"content": "x"}))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org POST release-notes must return 403");
    }

    #[tokio::test]
    async fn test_m8_generate_release_note_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("POST", format!("/v1/diffs/{diff_id}/release-notes/generate"),
                Some(serde_json::json!({}))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org generate release-note must return 403");
    }

    #[tokio::test]
    async fn test_m8_migration_guide_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("GET", format!("/v1/diffs/{diff_id}/migration-guide"), None))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org migration-guide must return 403");
    }

    #[tokio::test]
    async fn test_m8_create_acknowledgement_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("POST", "/v1/acknowledgements".into(),
                Some(serde_json::json!({"diff_id": diff_id, "acknowledged_by": "mallory"}))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org acknowledgement must return 403");
    }

    #[tokio::test]
    async fn test_m8_create_scan_cross_org_returns_403() {
        let pool = test_pool().await;
        let (service_id, _, _, _) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("POST", "/v1/scheduled-scans".into(),
                Some(serde_json::json!({
                    "service_id": service_id,
                    "spec_url": "https://example.com/openapi.yaml",
                    "interval_minutes": 60
                }))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org create scan must return 403");
    }

    #[tokio::test]
    async fn test_m8_create_subscription_cross_org_returns_403() {
        let pool = test_pool().await;
        let (service_id, _, _, _) = setup_beta_service(&pool).await;
        let consumer_id = insert_beta_consumer(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("POST", format!("/v1/services/{service_id}/subscriptions"),
                Some(serde_json::json!({"consumer_id": consumer_id}))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org create subscription must return 403");
    }

    #[tokio::test]
    async fn test_m8_generate_tests_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("POST", "/v1/generate-tests".into(),
                Some(serde_json::json!({"diff_id": diff_id}))))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org generate-tests must return 403");
    }

    #[tokio::test]
    async fn test_m8_list_diff_test_suites_cross_org_returns_403() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let resp = alpha_app(pool)
            .oneshot(alpha_req("GET", format!("/v1/diffs/{diff_id}/test-suites"), None))
            .await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "cross-org list diff test-suites must return 403");
    }

    // Desktop / no-auth single-tenant path: empty caller org must NOT trigger a
    // 403 even against a resource whose row carries a non-empty org_id.
    #[tokio::test]
    async fn test_m8_no_auth_desktop_path_not_forbidden() {
        let pool = test_pool().await;
        let (_, _, _, diff_id) = setup_beta_service(&pool).await;
        let note_id = insert_beta_release_note(&pool, &diff_id).await;
        // No JWT secret, no auth required → org resolves to "" → guard bypassed.
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/release-notes/{note_id}"))
            .body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "desktop/no-auth must still read the note (single-tenant)");
    }

    #[tokio::test]
    async fn test_auth_me_returns_claims_with_valid_cookie() {
        let secret = "test-oidc-secret-me-200-no-env";
        let pool = test_pool().await;
        // Pass secret at build time — no env var mutation, test-safe.
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some(secret.to_string()));

        let claims = JwtClaims {
            sub: "alice@example.com".into(),
            org_id: "example.com".into(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        };
        let token = sign_jwt(&claims, secret).expect("sign_jwt");

        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/auth/me")
            .header("cookie", format!("radar_session={token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["sub"], "alice@example.com");
        assert_eq!(json["org_id"], "example.com");
    }

    // ── E-3-T2: Policy Decision persistence ──────────────────────────────────

    #[tokio::test]
    async fn test_post_policy_decision_returns_201() {
        let pool = test_pool().await;
        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "verdict": "warn",
            "fail_mode": "warn",
            "actor": "radar-cli"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/policy-decisions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["id"].as_str().is_some(), "response must contain an id");
        assert_eq!(json["verdict"], "warn");
        assert_eq!(json["fail_mode"], "warn");
    }

    #[tokio::test]
    async fn test_post_policy_decision_with_diff_and_service() {
        let pool = test_pool().await;
        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);

        let diff_id = Uuid::new_v4().to_string();
        let service_id = Uuid::new_v4().to_string();
        let body = serde_json::json!({
            "diff_id": diff_id,
            "service_id": service_id,
            "verdict": "block",
            "fail_mode": "closed",
            "actor": "radar-action"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/policy-decisions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["diff_id"], diff_id);
        assert_eq!(json["service_id"], service_id);
        assert_eq!(json["verdict"], "block");
        assert_eq!(json["fail_mode"], "closed");
    }

    // ── E-7: Consumer upsert + collection evidence tests ────────────────────

    #[tokio::test]
    async fn consumer_upsert_creates_new_consumer() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({"name": "Billing Service Tests"});
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers/upsert")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["name"], "Billing Service Tests");
        assert_eq!(json["created"], true);
        assert!(json["id"].as_str().is_some_and(|s| !s.is_empty()));
    }

    #[tokio::test]
    async fn consumer_upsert_is_idempotent() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({"name": "Billing Service Tests"});

        // First call — creates
        let req1 = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers/upsert")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        let bytes1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let json1: serde_json::Value = serde_json::from_slice(&bytes1).unwrap();
        let first_id = json1["id"].as_str().unwrap().to_string();
        assert_eq!(json1["created"], true);

        // Second call — returns existing
        let req2 = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers/upsert")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
        let bytes2 = resp2.into_body().collect().await.unwrap().to_bytes();
        let json2: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        assert_eq!(json2["id"], first_id, "same consumer id must be returned on second upsert");
        assert_eq!(json2["created"], false);
    }

    #[tokio::test]
    async fn collection_evidence_written_and_accepted() {
        let pool = test_pool().await;

        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("coll-consumer-1")
        .bind("Billing Service Tests")
        .bind("")
        .bind("")
        .bind("")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!([
            {
                "consumer_id": "coll-consumer-1",
                "service_id":  "payments-api",
                "operation":   "GET /users/{id}",
                "field_path":  "phone",
                "evidence_uri": "file://collections/billing.postman_collection.json#Get User by ID"
            }
        ]);

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/evidence/collection")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["accepted"], 1);
        assert_eq!(json["inserted"], 1);
    }

    #[tokio::test]
    async fn collection_evidence_idempotent_no_duplicate_rows() {
        let pool = test_pool().await;

        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("coll-consumer-2")
        .bind("Billing Svc")
        .bind("")
        .bind("")
        .bind("")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!([
            {
                "consumer_id": "coll-consumer-2",
                "service_id":  "payments-api",
                "operation":   "GET /users/{id}",
                "field_path":  "phone"
            }
        ]);

        // First POST
        let req1 = HttpRequest::builder()
            .method("POST")
            .uri("/v1/evidence/collection")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let r1: serde_json::Value = serde_json::from_slice(
            &app.clone().oneshot(req1).await.unwrap().into_body().collect().await.unwrap().to_bytes()
        ).unwrap();
        assert_eq!(r1["inserted"], 1);

        // Second POST — same body → no new rows
        let req2 = HttpRequest::builder()
            .method("POST")
            .uri("/v1/evidence/collection")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let r2: serde_json::Value = serde_json::from_slice(
            &app.clone().oneshot(req2).await.unwrap().into_body().collect().await.unwrap().to_bytes()
        ).unwrap();
        assert_eq!(r2["inserted"], 0, "second POST must not insert duplicate evidence rows");

        // Verify only 1 row in impact_evidence
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM impact_evidence WHERE consumer_id = ? AND source_type = 'collection_file'",
        )
        .bind("coll-consumer-2")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "exactly 1 evidence row should exist after two identical scans");
    }

    // ── F-3: Acknowledgement workflow ─────────────────────────────────────────

    #[tokio::test]
    async fn acknowledgement_create_returns_201() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "acknowledged_by": "alice@example.com",
            "reason": "Consumers have been notified and updated"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/acknowledgements")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["id"].as_str().is_some(), "response must contain an id");
        assert_eq!(json["acknowledged_by"], "alice@example.com");
    }

    #[tokio::test]
    async fn acknowledgement_create_requires_acknowledged_by() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({ "reason": "missing acknowledged_by" });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/acknowledgements")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn acknowledgement_list_for_diff_returns_entries() {
        let pool = test_pool().await;

        let diff_id = Uuid::new_v4().to_string();
        let ack_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO acknowledgement (id, org_id, diff_id, acknowledged_by, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&ack_id)
        .bind("")
        .bind(&diff_id)
        .bind("bob@example.com")
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/acknowledgements"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["acknowledged_by"], "bob@example.com");
    }

    #[tokio::test]
    async fn acknowledgement_list_excludes_expired() {
        let pool = test_pool().await;

        let diff_id = Uuid::new_v4().to_string();
        let past = "2020-01-01T00:00:00Z";
        sqlx::query(
            "INSERT INTO acknowledgement (id, org_id, diff_id, acknowledged_by, expires_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("")
        .bind(&diff_id)
        .bind("carol@example.com")
        .bind(past)
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/acknowledgements"))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 0, "expired acknowledgements must not be returned");
    }

    // ── F-4: Catalog source CRUD ───────────────────────────────────────────────

    #[tokio::test]
    async fn catalog_source_create_returns_201() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "kind": "backstage",
            "name": "Internal Backstage",
            "url": "https://backstage.example.com"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/catalog-sources")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["id"].as_str().is_some());
        assert_eq!(json["kind"], "backstage");
        assert_eq!(json["name"], "Internal Backstage");
    }

    #[tokio::test]
    async fn catalog_source_create_validates_kind() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "kind": "unknown-kind",
            "name": "Bad Source",
            "url": "https://example.com"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/catalog-sources")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn catalog_source_list_returns_entries() {
        let pool = test_pool().await;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO catalog_source (id, org_id, kind, name, url, sync_interval_secs, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id).bind("").bind("codeowners").bind("Mono-repo CODEOWNERS")
        .bind("https://github.com/org/mono").bind(3600_i64).bind(&now)
        .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/catalog-sources")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entries = json["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["kind"], "codeowners");
        assert_eq!(entries[0]["name"], "Mono-repo CODEOWNERS");
    }

    // ── F-5: CODEOWNERS parser ─────────────────────────────────────────────────

    #[test]
    fn parse_codeowners_extracts_unique_owners() {
        let content = "
# Global owners
* @org/devops

# API team owns the spec
/api/ @org/api-team @alice

# Billing team
/services/billing/ @org/billing-team

# Same as api-team appears again
/proto/ @org/api-team
";
        let owners = parse_codeowners(content);
        assert!(owners.contains(&"org/devops".to_string()));
        assert!(owners.contains(&"org/api-team".to_string()));
        assert!(owners.contains(&"org/billing-team".to_string()));
        assert!(owners.contains(&"alice".to_string()));
        // api-team appears twice but must be de-duped
        assert_eq!(owners.iter().filter(|o| *o == "org/api-team").count(), 1);
    }

    #[test]
    fn parse_codeowners_skips_comments_and_empty() {
        let content = "
# This is a comment
   # Indented comment

/docs/ @org/docs-team
";
        let owners = parse_codeowners(content);
        assert_eq!(owners, vec!["org/docs-team"]);
    }

    #[test]
    fn parse_codeowners_skips_entries_without_owners() {
        // Pattern with no @ owners — should not produce empty strings
        let content = "/ignored/path/\n/real-path/ @org/backend";
        let owners = parse_codeowners(content);
        assert_eq!(owners, vec!["org/backend"]);
    }

    // ── G-2: OTLP trace ingest ────────────────────────────────────────────────

    #[tokio::test]
    async fn otlp_traces_accepted_and_creates_usage_event() {
        let pool = test_pool().await;

        // Register a consumer and service so sampling config lookup doesn't fail.
        sqlx::query("INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)")
            .bind("c-otlp").bind("otlp-consumer").bind("").bind("").bind("")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("s-otlp").bind("otlp-service").bind("").bind("").bind("openapi")
            .execute(&pool).await.unwrap();

        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);
        let body = serde_json::json!({
            "resourceSpans": [{
                "resource": { "attributes": [] },
                "scopeSpans": [{
                    "spans": [{
                        "spanId": "abc",
                        "kind": 3,
                        "attributes": [
                            { "key": "http.method", "value": { "stringValue": "GET" } },
                            { "key": "http.route",  "value": { "stringValue": "/users/{id}" } },
                            { "key": "radar.consumer_id", "value": { "stringValue": "c-otlp" } },
                            { "key": "radar.service_id",  "value": { "stringValue": "s-otlp" } }
                        ]
                    }]
                }]
            }]
        });
        let req = HttpRequest::builder()
            .method("POST").uri("/v1/otlp/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["accepted"], 1);
    }

    #[tokio::test]
    async fn otlp_traces_skips_server_spans() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let body = serde_json::json!({
            "resourceSpans": [{
                "resource": { "attributes": [] },
                "scopeSpans": [{
                    "spans": [{
                        "spanId": "xyz",
                        "kind": 2, // SERVER span — should be skipped
                        "attributes": [
                            { "key": "http.method", "value": { "stringValue": "GET" } },
                            { "key": "http.route",  "value": { "stringValue": "/ping" } },
                            { "key": "radar.consumer_id", "value": { "stringValue": "c1" } },
                            { "key": "radar.service_id",  "value": { "stringValue": "s1" } }
                        ]
                    }]
                }]
            }]
        });
        let req = HttpRequest::builder()
            .method("POST").uri("/v1/otlp/v1/traces")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["accepted"], 0);
    }

    // ── G-2: normalise_path ───────────────────────────────────────────────────

    #[test]
    fn normalise_path_replaces_numeric_segments() {
        assert_eq!(normalise_path("/users/123"), "/users/{id}");
        assert_eq!(normalise_path("/orders/456/items/7"), "/orders/{id}/items/{id}");
        assert_eq!(normalise_path("/users/{id}"), "/users/{id}"); // already normalised
        assert_eq!(normalise_path("/health"), "/health");
    }

    // ── G-3: Gateway log ingest ───────────────────────────────────────────────

    #[tokio::test]
    async fn gateway_logs_accepted() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let body = serde_json::json!([
            { "method": "POST", "path": "/payments", "consumer_id": "c1", "service_id": "s1", "status_code": 201 },
            { "method": "GET",  "path": "/users/99",  "consumer_id": "c1", "service_id": "s1" }
        ]);
        let req = HttpRequest::builder()
            .method("POST").uri("/v1/gateway/logs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["accepted"], 2);
    }

    #[tokio::test]
    async fn gateway_logs_path_normalised() {
        let pool = test_pool().await;

        // Insert parent rows required by usage_event FK constraints.
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("s-norm").bind("s-norm").bind("").bind("").bind("openapi")
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("c-norm").bind("c-norm").bind("").bind("").bind("")
        .execute(&pool).await.unwrap();

        // Insert a usage event via gateway log and verify the stored operation is normalised.
        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);
        let body = serde_json::json!([
            { "method": "GET", "path": "/users/42", "consumer_id": "c-norm", "service_id": "s-norm" }
        ]);
        let req = HttpRequest::builder()
            .method("POST").uri("/v1/gateway/logs")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let _ = app.oneshot(req).await.unwrap();

        let stored: Option<String> = sqlx::query_scalar(
            "SELECT operation FROM usage_event WHERE consumer_id = 'c-norm' LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(stored.as_deref(), Some("GET /users/{id}"));
    }

    // ── G-6: Sampling controls ────────────────────────────────────────────────

    #[tokio::test]
    async fn sampling_put_and_get_roundtrip() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let put_req = HttpRequest::builder()
            .method("PUT").uri("/v1/services/my-svc/sampling")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sample_rate":0.5,"field_deny_list":["password","secret"]}"#))
            .unwrap();
        let put_resp = app.clone().oneshot(put_req).await.unwrap();
        assert_eq!(put_resp.status(), StatusCode::OK);

        let get_req = HttpRequest::builder()
            .method("GET").uri("/v1/services/my-svc/sampling")
            .body(Body::empty()).unwrap();
        let get_resp = app.oneshot(get_req).await.unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);
        let bytes = get_resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!((json["sample_rate"].as_f64().unwrap() - 0.5).abs() < 0.001);
        let deny = json["field_deny_list"].as_array().unwrap();
        assert!(deny.iter().any(|v| v.as_str() == Some("password")));
    }

    #[tokio::test]
    async fn sampling_rejects_invalid_rate() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("PUT").uri("/v1/services/svc/sampling")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"sample_rate":1.5}"#)).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ── G-2: field deny-list helper ───────────────────────────────────────────

    #[test]
    fn field_in_deny_list_matches_exact() {
        assert!(field_in_deny_list("password", "password,secret"));
        assert!(field_in_deny_list("secret", "password,secret"));
        assert!(!field_in_deny_list("username", "password,secret"));
    }

    #[test]
    fn field_in_deny_list_supports_glob() {
        assert!(field_in_deny_list("user.token", "**.token"));
        assert!(!field_in_deny_list("auth.refresh_token", "**.token,password"));
        assert!(!field_in_deny_list("user.name", "**.token"));
    }

    // ── G-7: Evidence coverage ────────────────────────────────────────────────

    #[tokio::test]
    async fn evidence_coverage_returns_entries() {
        let pool = test_pool().await;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO impact_evidence \
             (id, org_id, diff_id, consumer_id, source_type, confidence, observed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("")
        .bind("diff-cov-1")
        .bind("consumer-cov-1")
        .bind("runtime_usage")
        .bind("high")
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET").uri("/v1/evidence/coverage")
            .body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        // Response is a flat array (no envelope) — matches CoverageRow[] in the UI.
        let entries: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entries = entries.as_array().unwrap();
        assert!(!entries.is_empty());
        let entry = &entries[0];
        assert_eq!(entry["consumer_id"], "consumer-cov-1");
        assert_eq!(entry["is_stale"], false); // just inserted = not stale
    }

    // ── F+: Evolution rule helpers ───────────────────────────────────────────

    #[test]
    fn path_matches_empty_pattern_matches_all() {
        assert!(path_matches("", "users.id"));
        assert!(path_matches("", ""));
        assert!(path_matches("", "a.b.c.d"));
    }

    #[test]
    fn path_matches_exact() {
        assert!(path_matches("users.id", "users.id"));
        assert!(!path_matches("users.id", "users.email"));
        assert!(!path_matches("users.id", "accounts.id"));
    }

    #[test]
    fn path_matches_single_wildcard() {
        assert!(path_matches("users.*", "users.id"));
        assert!(path_matches("users.*", "users.email"));
        assert!(!path_matches("users.*", "users.profile.avatar"));
        assert!(!path_matches("*.id", "users.profile.id")); // * is single-segment
    }

    #[test]
    fn path_matches_double_wildcard() {
        assert!(path_matches("users.**", "users.profile.avatar"));
        assert!(path_matches("users.**", "users.id"));
        assert!(path_matches("users.**", "users.a.b.c"));
        assert!(!path_matches("users.**", "accounts.id"));
    }

    #[test]
    fn severity_downgrade_logic() {
        assert!(is_severity_downgrade("breaking", "non_breaking_risky"));
        assert!(is_severity_downgrade("breaking", "safe"));
        assert!(is_severity_downgrade("non_breaking_risky", "safe"));
        assert!(!is_severity_downgrade("safe", "safe"));
        assert!(!is_severity_downgrade("safe", "non_breaking_risky"));
        assert!(!is_severity_downgrade("non_breaking_risky", "breaking"));
    }

    #[test]
    fn apply_evolution_rules_downgrades_matching_change() {
        let changes = vec![json!({
            "path": "users.legacy_token",
            "kind": "field_removed",
            "severity": "breaking",
            "description": null,
        })];
        let rules = vec![(
            "rule-1".to_string(),
            "Allow removing legacy fields".to_string(),
            Some("users.*".to_string()),
            "field_removed".to_string(),
            "safe".to_string(),
        )];
        let result = apply_evolution_rules(changes, &rules);
        assert_eq!(result[0]["severity"], "safe");
        assert_eq!(result[0]["applied_rule"]["id"], "rule-1");
        assert_eq!(result[0]["applied_rule"]["original_severity"], "breaking");
    }

    #[test]
    fn apply_evolution_rules_ignores_non_matching_kind() {
        let changes = vec![json!({
            "path": "users.id",
            "kind": "type_changed",
            "severity": "breaking",
            "description": null,
        })];
        let rules = vec![(
            "rule-1".to_string(),
            "Safe field removal".to_string(),
            None,
            "field_removed".to_string(), // different kind
            "safe".to_string(),
        )];
        let result = apply_evolution_rules(changes, &rules);
        assert_eq!(result[0]["severity"], "breaking");
        assert!(result[0].get("applied_rule").is_none());
    }

    #[test]
    fn apply_evolution_rules_does_not_upgrade_severity() {
        let changes = vec![json!({
            "path": "users.id",
            "kind": "field_added",
            "severity": "safe",
            "description": null,
        })];
        let rules = vec![(
            "rule-1".to_string(),
            "Make adds risky".to_string(),
            None,
            "field_added".to_string(),
            "non_breaking_risky".to_string(), // would be an upgrade from safe → not allowed
        )];
        let result = apply_evolution_rules(changes, &rules);
        assert_eq!(result[0]["severity"], "safe"); // unchanged
        assert!(result[0].get("applied_rule").is_none());
    }

    // ── F+: Evolution rule CRUD (integration) ────────────────────────────────

    #[tokio::test]
    async fn evolution_rule_create_returns_201() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/evolution-rules")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"Allow safe adds","change_kind":"field_added","severity_override":"safe"}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["id"].as_str().is_some());
        assert_eq!(json["change_kind"], "field_added");
        assert_eq!(json["severity_override"], "safe");
        assert_eq!(json["enabled"], true);
    }

    #[tokio::test]
    async fn evolution_rule_create_validates_change_kind() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/evolution-rules")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"Bad","change_kind":"unknown_kind","severity_override":"safe"}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn evolution_rule_create_validates_severity_override() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/evolution-rules")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"name":"Bad","change_kind":"field_removed","severity_override":"breaking"}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn evolution_rule_list_returns_entries() {
        let pool = test_pool().await;

        // Insert a rule directly so we don't need app.clone()
        sqlx::query(
            "INSERT INTO evolution_rule (id, org_id, name, change_kind, severity_override) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("")
        .bind("Test Rule")
        .bind("enum_value_added")
        .bind("safe")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/evolution-rules")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let entries = json["entries"].as_array().unwrap();
        assert!(!entries.is_empty());
        assert_eq!(entries[0]["change_kind"], "enum_value_added");
    }

    #[tokio::test]
    async fn evolution_rule_delete_removes_rule() {
        let pool = test_pool().await;
        let rule_id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO evolution_rule (id, org_id, name, change_kind, severity_override) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&rule_id)
        .bind("")
        .bind("To Delete")
        .bind("field_removed")
        .bind("non_breaking_risky")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("DELETE")
            .uri(format!("/v1/evolution-rules/{rule_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    // -------------------------------------------------------------------------
    // H-2: templates_from_changes
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_templates_field_removed() {
        let changes = vec![json!({
            "path": "GET /users/{id} \u{2192} response.email",
            "kind": "field_removed",
            "severity": "breaking",
        })];
        let suite = templates_from_changes(&changes, &[]);
        let cases = suite["test_cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c["name"].as_str().unwrap_or("").contains("email")));
    }

    #[tokio::test]
    async fn test_templates_required_changed() {
        let changes = vec![json!({
            "path": "POST /orders \u{2192} body.amount",
            "kind": "required_changed",
            "severity": "breaking",
        })];
        let suite = templates_from_changes(&changes, &[]);
        let cases = suite["test_cases"].as_array().unwrap();
        assert!(cases.iter().any(|c| {
            let name = c["name"].as_str().unwrap_or("");
            name.contains("[NEGATIVE]") && name.contains("amount")
        }));
    }

    #[tokio::test]
    async fn test_templates_operation_removed() {
        let changes = vec![json!({
            "path": "DELETE /resources/{id}",
            "kind": "operation_removed",
            "severity": "breaking",
        })];
        let suite = templates_from_changes(&changes, &[]);
        let cases = suite["test_cases"].as_array().unwrap();
        assert!(cases.iter().any(|c| {
            c["expected_status"].as_u64() == Some(404)
        }));
    }

    #[tokio::test]
    async fn test_templates_unknown_kind_yields_smoke_test() {
        let changes = vec![json!({
            "path": "GET /healthz",
            "kind": "some_future_kind",
            "severity": "safe",
        })];
        let suite = templates_from_changes(&changes, &[]);
        let cases = suite["test_cases"].as_array().unwrap();
        // Falls through to smoke test
        assert!(!cases.is_empty());
    }

    // -------------------------------------------------------------------------
    // H-1: diff-based test generation (use_templates)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_generate_tests_from_diff_templates() {
        let pool = test_pool().await;

        // Seed: service + two spec versions + diff + change
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("svc-h1").bind("SvcH1").bind("").bind("").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-h1a").bind("svc-h1").bind("abc").bind("openapi: '3.0'").bind("2026-01-01T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-h1b").bind("svc-h1").bind("def").bind("openapi: '3.0'").bind("2026-01-02T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("diff-h1").bind("sv-h1a").bind("sv-h1b").bind("").bind("2026-01-02T00:00:00Z")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("chg-h1").bind("diff-h1")
        .bind("GET /items/{id} \u{2192} response.price")
        .bind("field_removed").bind("breaking")
        .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let body = serde_json::to_vec(&json!({
            "diff_id": "diff-h1",
            "use_templates": true,
        })).unwrap();
        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/generate-tests")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let val: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(val["diff_id"].as_str(), Some("diff-h1"));
        assert!(val["test_count"].as_i64().unwrap_or(0) > 0);
    }

    // -------------------------------------------------------------------------
    // H-4: release-note status workflow
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_release_note_status_transition() {
        let pool = test_pool().await;

        // Seed required rows.
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("svc-rn").bind("SvcRN").bind("").bind("").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-rn1").bind("svc-rn").bind("v1").bind("").bind("2026-01-01T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-rn2").bind("svc-rn").bind("v2").bind("").bind("2026-01-02T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("diff-rn").bind("sv-rn1").bind("sv-rn2").bind("").bind("2026-01-02T00:00:00Z")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO release_note (id, diff_id, content, created_at, status) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("rn-1").bind("diff-rn").bind("# Release notes").bind("2026-01-02T00:00:00Z").bind("draft")
        .execute(&pool).await.unwrap();

        let app = build_router(pool.clone(), None, 4 * 1024 * 1024, false, None);

        // draft → reviewed
        let body = serde_json::to_vec(&json!({ "status": "reviewed" })).unwrap();
        let req = HttpRequest::builder()
            .method("PATCH")
            .uri("/v1/release-notes/rn-1/status")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let val: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(val["status"].as_str(), Some("reviewed"));
    }

    #[tokio::test]
    async fn test_release_note_invalid_transition_rejected() {
        let pool = test_pool().await;

        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("svc-rn2").bind("SvcRN2").bind("").bind("").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-rn3").bind("svc-rn2").bind("v1").bind("").bind("2026-01-01T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-rn4").bind("svc-rn2").bind("v2").bind("").bind("2026-01-02T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("diff-rn2").bind("sv-rn3").bind("sv-rn4").bind("").bind("2026-01-02T00:00:00Z")
        .execute(&pool).await.unwrap();

        // Insert a 'published' note — cannot go back to 'reviewed'.
        sqlx::query(
            "INSERT INTO release_note (id, diff_id, content, created_at, status) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("rn-2").bind("diff-rn2").bind("# Notes").bind("2026-01-02T00:00:00Z").bind("published")
        .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let body = serde_json::to_vec(&json!({ "status": "reviewed" })).unwrap();
        let req = HttpRequest::builder()
            .method("PATCH")
            .uri("/v1/release-notes/rn-2/status")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    // -------------------------------------------------------------------------
    // H-3: migration guide
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_migration_guide_returns_markdown() {
        let pool = test_pool().await;

        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("svc-mg").bind("Payments").bind("").bind("").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-mg1").bind("svc-mg").bind("v1.0").bind("").bind("2026-01-01T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, spec_yaml, captured_at, spec_format) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("sv-mg2").bind("svc-mg").bind("v2.0").bind("").bind("2026-01-02T00:00:00Z").bind("openapi")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("diff-mg").bind("sv-mg1").bind("sv-mg2").bind("").bind("2026-01-02T00:00:00Z")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("chg-mg").bind("diff-mg")
        .bind("GET /charges/{id} \u{2192} response.amount")
        .bind("field_removed").bind("breaking")
        .execute(&pool).await.unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/diffs/diff-mg/migration-guide")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("Migration Guide"));
        assert!(body.contains("Payments"));
        assert!(body.contains("field_removed"));
    }

    // ── TD-2: TestClient fixture — new coverage of untested edge cases ─────────

    #[tokio::test]
    async fn gateway_logs_batch_limit_rejected() {
        let client = test_helpers::TestClient::new(test_pool().await);
        let entries: Vec<serde_json::Value> = (0..5001)
            .map(|i| serde_json::json!({"consumer_id": format!("c{i}"), "service_id": format!("s{i}"), "method": "GET", "path": "/ping"}))
            .collect();
        let resp = client.post_json("/v1/gateway/logs", &serde_json::Value::Array(entries)).await;
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(resp.json()["error"], "batch too large, max 5000");
    }

    #[tokio::test]
    async fn otlp_traces_resolves_consumer_by_service_name() {
        let pool = test_pool().await;

        // Consumer registered with a name matching the OTLP resource service.name attribute.
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("c-svc-name").bind("payments-svc").bind("").bind("").bind("")
        .execute(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("s-svc-name").bind("ledger-svc").bind("").bind("").bind("openapi")
        .execute(&pool).await.unwrap();

        let client = test_helpers::TestClient::new(pool);
        let resp = client.post_json(
            "/v1/otlp/v1/traces",
            &serde_json::json!({
                "resourceSpans": [{
                    "resource": {
                        "attributes": [
                            {"key": "service.name", "value": {"stringValue": "payments-svc"}}
                        ]
                    },
                    "scopeSpans": [{
                        "spans": [{
                            "kind": 3,
                            "attributes": [
                                {"key": "radar.service_id", "value": {"stringValue": "s-svc-name"}},
                                {"key": "http.method",      "value": {"stringValue": "POST"}},
                                {"key": "http.route",       "value": {"stringValue": "/charges"}}
                            ]
                        }]
                    }]
                }]
            }),
        ).await;

        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        assert_eq!(resp.json()["accepted"], 1);
    }

    // -----------------------------------------------------------------------
    // TD-4: token-based rate-limit key tests
    // -----------------------------------------------------------------------

    fn with_peer(req: &mut Request, addr: &str) {
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            addr.parse::<std::net::SocketAddr>().unwrap(),
        ));
    }

    #[test]
    fn client_key_ignores_bearer_token_no_bypass() {
        // A random Bearer token must NOT create its own bucket — otherwise an
        // attacker mints a fresh token per request to bypass the limiter.
        let mut a = axum::http::Request::builder()
            .header("authorization", "Bearer randomtoken-AAAA")
            .body(axum::body::Body::empty())
            .unwrap();
        let mut b = axum::http::Request::builder()
            .header("authorization", "Bearer randomtoken-BBBB")
            .body(axum::body::Body::empty())
            .unwrap();
        with_peer(&mut a, "203.0.113.7:40000");
        with_peer(&mut b, "203.0.113.7:40001");
        // Same peer IP, different tokens → same bucket.
        assert_eq!(client_key(&a, false), "ip:203.0.113.7");
        assert_eq!(client_key(&a, false), client_key(&b, false));
    }

    #[test]
    fn client_key_uses_peer_addr() {
        let mut req = axum::http::Request::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        with_peer(&mut req, "198.51.100.9:1234");
        assert_eq!(client_key(&req, false), "ip:198.51.100.9");
    }

    #[test]
    fn client_key_ignores_xff_when_proxy_untrusted() {
        // Spoofed XFF must be ignored without RADAR_TRUST_PROXY; the real peer wins.
        let mut req = axum::http::Request::builder()
            .header("x-forwarded-for", "10.0.0.42")
            .body(axum::body::Body::empty())
            .unwrap();
        with_peer(&mut req, "198.51.100.9:1234");
        assert_eq!(client_key(&req, false), "ip:198.51.100.9");
    }

    #[test]
    fn client_key_uses_xff_when_proxy_trusted() {
        let req = axum::http::Request::builder()
            .header("x-forwarded-for", "10.0.0.42, 172.16.0.1")
            .body(axum::body::Body::empty())
            .unwrap();
        // Leftmost entry is the originating client behind a trusted proxy chain.
        assert_eq!(client_key(&req, true), "ip:10.0.0.42");
    }

    #[test]
    fn client_key_unknown_when_no_peer_or_headers() {
        let req = axum::http::Request::builder()
            .body(axum::body::Body::empty())
            .unwrap();
        assert_eq!(client_key(&req, false), "unknown");
    }

    // J-6 / Phase-3: POST generate returns pending job; GET generate-status returns completed content
    #[tokio::test]
    async fn test_generate_release_note_async_job() {
        let pool = test_pool().await;

        sqlx::query("INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("svc-rn").bind("RN Svc").bind("").bind("team").bind("openapi")
            .execute(&pool).await.unwrap();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("sv-rn-a").bind("svc-rn").bind("v1").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("sv-rn-b").bind("svc-rn").bind("v2").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, NULL, ?)")
            .bind("diff-rn").bind("sv-rn-a").bind("sv-rn-b").bind(&now)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("chg-rn").bind("diff-rn").bind("GET /users → phone").bind("field_removed").bind("breaking").bind(Option::<String>::None)
            .execute(&pool).await.unwrap();

        let client = test_helpers::TestClient::new(pool.clone());

        // POST → should return 201 with generation_status pending.
        let resp = client.post_json("/v1/diffs/diff-rn/release-notes/generate", &serde_json::json!({})).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.json();
        let note_id = body["id"].as_str().expect("id must be present").to_owned();
        assert_eq!(body["generation_status"], "pending");
        assert!(body.get("content").and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true),
                "content should not be returned in pending state");

        // Background task completes almost instantly (template, no I/O).
        // Poll up to 1 s to be safe.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1000);
        let gen_status: String;
        let mut final_content = String::new();
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
            let status_resp = client.get(&format!("/v1/release-notes/{note_id}/generate-status")).await;
            assert_eq!(status_resp.status(), StatusCode::OK);
            let status_body = status_resp.json();
            let status = status_body["generation_status"].as_str().unwrap_or("").to_owned();
            if status == "completed" {
                final_content = status_body["content"].as_str().unwrap_or("").to_owned();
                gen_status = status;
                break;
            }
            if status == "failed" || std::time::Instant::now() >= deadline {
                gen_status = status;
                break;
            }
        }
        assert_eq!(gen_status, "completed", "generation did not complete in time");
        assert!(final_content.contains("field_removed"),
                "expected generated content to mention field_removed, got: {final_content}");
    }

    // Phase-4 / STRIDE: SQL injection attempt in path param does not cause 500
    #[tokio::test]
    async fn test_stride_injection_in_path_returns_not_found_not_500() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        // A typical SQL injection string as a path parameter.
        let resp = client.get("/v1/diffs/1%27%20OR%20%271%27%3D%271").await;
        assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR,
                   "SQL injection attempt must not cause 500");
    }

    // Phase-4 / STRIDE: unauthenticated request when JWT_SECRET is configured returns 401
    #[tokio::test]
    async fn test_stride_unauthenticated_request_returns_401_when_jwt_required() {
        let pool = test_pool().await;
        // Build a router with JWT enabled (non-empty secret).
        let app = build_router(pool, None, 4 * 1024 * 1024, false, Some("test-secret".into()));
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/v1/services")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
                   "unauthenticated requests must be rejected with 401 when JWT is enabled");
    }

    // Phase-4 / STRIDE: error responses do not leak internal details (DB path, connection string)
    #[tokio::test]
    async fn test_stride_error_bodies_do_not_leak_db_path() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        // Force a 404 on a non-existent resource — error body must not contain DB internals.
        let resp = client.get("/v1/diffs/nonexistent-diff-id-xyz").await;
        let body_text = resp.text().to_lowercase();
        assert!(!body_text.contains("sqlite"),           "error must not reveal DB engine");
        assert!(!body_text.contains("drift.db"),         "error must not reveal DB file path");
        assert!(!body_text.contains("sqlx"),             "error must not reveal ORM details");
        assert!(!body_text.contains("connection string"), "error must not reveal DB credentials");
    }

    // Phase-4 / STRIDE: SSRF — redirect to private IP from a 3xx response is blocked
    // (reqwest is configured with redirect::Policy::none() so 3xx are not followed)
    #[tokio::test]
    async fn test_stride_webhook_disallows_redirect_to_private_ip() {
        // We test at the URL-validation layer: a private IP must be rejected on registration.
        // Redirect bypass at delivery time is prevented by Policy::none() (verified in unit tests).
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.post_json(
            "/v1/webhooks",
            &serde_json::json!({ "url": "https://10.0.0.1/hook", "events": ["diff.created"] }),
        ).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY,
                   "webhook to private IP must be rejected");
    }

    // Phase-5 / Story 3: org isolation — data inserted for one org is not visible to another
    #[tokio::test]
    async fn test_org_isolation_audit_events_scoped_to_org() {
        let pool = test_pool().await;
        // Insert an audit event directly for org "org-a".
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO audit_event (id, org_id, actor, action, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("evt-org-a").bind("org-a").bind("alice").bind("test.action").bind(&now)
        .execute(&pool).await.unwrap();

        // Query without JWT → org_id resolves to "" → must see zero events.
        let client = test_helpers::TestClient::new(pool);
        let resp = client.get("/v1/audit-events").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.json();
        let entries = body["entries"].as_array().unwrap();
        assert!(entries.is_empty(),
                "audit events for org-a must not be visible to the default empty-org session");
    }

    // Phase-5 / Story 3: CSV run state machine — cancelling a completed run returns 404
    #[tokio::test]
    async fn test_cancel_completed_csv_run_returns_404() {
        let pool = test_pool().await;
        // Insert a completed job directly.
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO csv_run_job \
             (id, org_id, name, request_json, status, total_rows, completed_rows, error_count, created_at) \
             VALUES ('job-done', '', 'done-run', '{}', 'completed', 1, 1, 0, ?)",
        )
        .bind(&now).execute(&pool).await.unwrap();

        // DELETE on a completed job must return 404 (the WHERE status IN ('pending','running') guard).
        let client = test_helpers::TestClient::new(pool);
        let resp = client.delete("/v1/csv-runs/job-done").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND,
                   "cancelling a completed job must return 404 (state machine guard)");
    }

    // Phase-5 / Story 3: JWT-required endpoints return 401, not 500, without credentials
    #[tokio::test]
    async fn test_jwt_required_endpoints_return_401() {
        let pool = test_pool().await;
        // require_auth=true enforces JWT validation on every /v1 request.
        let app = build_router(pool, None, 4 * 1024 * 1024, true, Some("test-secret".to_string()));
        // POST /v1/consumers without a token must be rejected.
        let req = HttpRequest::builder()
            .method("POST").uri("/v1/consumers")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"Test","repo_url":"","owner_team":"t","contact":"t@t"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
                   "protected endpoint without JWT must return 401, not 500");
    }

    // J-2: creating a consumer without repo_url must succeed (repo_url is optional)
    #[tokio::test]
    async fn test_create_consumer_without_repo_url() {
        let pool = test_pool().await;
        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "name": "QA Team",
            "repo_url": "",
            "owner_team": "Quality",
            "contact": "qa@acme.com"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/consumers")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["id"].is_string());
        assert_eq!(json["name"], "QA Team");
    }

    // J-1: compare two raw spec strings and receive a persisted diff
    #[tokio::test]
    async fn test_compare_specs_returns_diff_id() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("svc-cmp")
        .bind("Compare Svc")
        .bind("https://github.com/acme/cmp")
        .bind("team-cmp")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        // v1 exposes GET /users with a `phone` response field.
        let base_spec = concat!(
            "openapi: \"3.0.0\"\ninfo:\n  title: T\n  version: \"1\"\npaths:\n",
            "  /users:\n    get:\n      responses:\n        \"200\":\n",
            "          description: OK\n",
            "          content:\n            application/json:\n              schema:\n",
            "                type: object\n                properties:\n",
            "                  phone:\n                    type: string\n"
        );
        // v2 removes `phone` — breaking change.
        let head_spec = concat!(
            "openapi: \"3.0.0\"\ninfo:\n  title: T\n  version: \"2\"\npaths:\n",
            "  /users:\n    get:\n      responses:\n        \"200\":\n",
            "          description: OK\n",
            "          content:\n            application/json:\n              schema:\n",
            "                type: object\n                properties:\n",
            "                  email:\n                    type: string\n"
        );

        let body = serde_json::json!({
            "base_spec": base_spec,
            "head_spec": head_spec,
            "spec_format": "openapi",
            "base_ref": "v1",
            "head_ref": "v2"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/services/svc-cmp/diffs/compare")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["diff_id"].is_string(), "diff_id must be a string");
        assert!(json["changes_count"].as_i64().unwrap_or(0) > 0, "must detect changes");
        assert!(json["breaking_count"].as_i64().unwrap_or(0) > 0, "must detect breaking changes");
    }

    #[tokio::test]
    async fn test_compare_specs_bad_yaml_returns_422() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("svc-cmp2")
        .bind("Compare Svc2")
        .bind("https://github.com/acme/cmp2")
        .bind("team-cmp2")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();

        let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

        let body = serde_json::json!({
            "base_spec": "not: valid: openapi: [[[",
            "head_spec": "also: bad",
            "spec_format": "openapi",
            "base_ref": "v1",
            "head_ref": "v2"
        });

        let req = HttpRequest::builder()
            .method("POST")
            .uri("/v1/services/svc-cmp2/diffs/compare")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"], "parse_error");
        assert!(json["spec"].is_string());
    }

    // -----------------------------------------------------------------------
    // EPIC K — Webhooks (TD-NEW-1)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_webhook_create_rejects_http_scheme() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json("/v1/webhooks", &serde_json::json!({"url": "http://example.com/hook"}))
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_webhook_create_rejects_rfc1918_ip_literal() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json("/v1/webhooks", &serde_json::json!({"url": "https://192.168.1.100/hook"}))
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_webhook_create_rejects_loopback_ip() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json("/v1/webhooks", &serde_json::json!({"url": "https://127.0.0.1/hook"}))
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_webhook_list_returns_empty_array_initially() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.get("/v1/webhooks").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.json(), serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_webhook_delete_unknown_id_returns_404() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.delete("/v1/webhooks/no-such-webhook").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_webhook_deliveries_for_unknown_webhook_returns_404() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.get("/v1/webhooks/no-such-webhook/deliveries").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // EPIC K — Scheduled Scans (TD-NEW-1)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_scan_create_rejects_interval_below_15_minutes() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "svc-test",
                    "spec_url": "https://93.184.216.34/openapi.yaml",
                    "interval_minutes": 10
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_scan_create_rejects_empty_service_id() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "",
                    "spec_url": "https://93.184.216.34/openapi.yaml",
                    "interval_minutes": 30
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_scan_create_rejects_http_spec_url() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "svc-scan",
                    "spec_url": "http://internal.example.com/openapi.yaml",
                    "interval_minutes": 30
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_scan_create_rejects_rfc1918_spec_url() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "svc-scan",
                    "spec_url": "https://10.0.0.1/openapi.yaml",
                    "interval_minutes": 30
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_scan_list_returns_empty_array_initially() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.get("/v1/scheduled-scans").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.json(), serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_scan_delete_unknown_id_returns_404() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.delete("/v1/scheduled-scans/no-such-scan").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_scan_run_history_returns_empty_array_initially() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.get("/v1/scheduled-scans/history").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.json(), serde_json::json!([]));
    }

    // --- webhook happy-path ---

    #[tokio::test]
    async fn test_webhook_create_valid_returns_201() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json(
                "/v1/webhooks",
                &serde_json::json!({"url": "https://93.184.216.34/incoming"}),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.json();
        assert!(body["id"].as_str().is_some());
        assert_eq!(body["url"], "https://93.184.216.34/incoming");
    }

    #[tokio::test]
    async fn test_webhook_create_duplicate_returns_200() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let payload = serde_json::json!({"url": "https://93.184.216.34/dupe"});
        let r1 = client.post_json("/v1/webhooks", &payload).await;
        assert_eq!(r1.status(), StatusCode::CREATED);
        let r2 = client.post_json("/v1/webhooks", &payload).await;
        assert_eq!(r2.status(), StatusCode::OK);
        assert_eq!(r1.json()["id"], r2.json()["id"]);
    }

    #[tokio::test]
    async fn test_webhook_list_shows_created_webhook() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        client
            .post_json(
                "/v1/webhooks",
                &serde_json::json!({"url": "https://93.184.216.34/list-test"}),
            )
            .await;
        let resp = client.get("/v1/webhooks").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let arr = resp.json();
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["url"], "https://93.184.216.34/list-test");
    }

    #[tokio::test]
    async fn test_webhook_delete_existing_returns_204() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let created = client
            .post_json(
                "/v1/webhooks",
                &serde_json::json!({"url": "https://93.184.216.34/to-delete"}),
            )
            .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let id = created.json()["id"].as_str().unwrap().to_string();
        let del = client.delete(&format!("/v1/webhooks/{id}")).await;
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
        let list = client.get("/v1/webhooks").await;
        assert_eq!(list.json().as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_webhook_test_fire_returns_202() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let created = client
            .post_json(
                "/v1/webhooks",
                &serde_json::json!({"url": "https://93.184.216.34/ping-target"}),
            )
            .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let id = created.json()["id"].as_str().unwrap().to_string();
        let resp = client
            .post_json(&format!("/v1/webhooks/{id}/test"), &serde_json::json!({}))
            .await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_webhook_deliveries_for_existing_webhook_returns_array() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let created = client
            .post_json(
                "/v1/webhooks",
                &serde_json::json!({"url": "https://93.184.216.34/delivery-check"}),
            )
            .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let id = created.json()["id"].as_str().unwrap().to_string();
        let resp = client.get(&format!("/v1/webhooks/{id}/deliveries")).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.json().as_array().is_some());
    }

    // Phase-3 / K-SMOKE: webhook delivery with in-process echo server
    // Verifies that dispatch_diff_event fires a real HTTP POST to the registered URL.
    #[tokio::test]
    async fn test_webhook_delivery_reaches_echo_server() {
        let echo = test_helpers::spawn_echo_server().await;
        let url = format!("http://127.0.0.1:{}/hook", echo.addr.port());

        // radar-api SSRF guard blocks non-HTTPS and RFC1918 IPs at *registration* time.
        // For this test we bypass registration and insert the webhook directly so the
        // delivery path is tested without interference from the input-validation guard.
        let pool = test_pool().await;
        let wh_id  = uuid::Uuid::new_v4().to_string();
        let secret = uuid::Uuid::new_v4().to_string();
        let now    = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO webhook (id, org_id, url, events, secret, active, created_at) \
             VALUES (?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(&wh_id).bind("").bind(&url)
        .bind("diff.created").bind(&secret).bind(&now)
        .execute(&pool).await.unwrap();

        // Insert a diff to trigger webhook dispatch.
        sqlx::query("INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("svc-wh").bind("WH Svc").bind("").bind("team").bind("openapi")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("sv-wh-a").bind("svc-wh").bind("v1").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)")
            .bind("sv-wh-b").bind("svc-wh").bind("v2").bind(&now).bind("openapi")
            .execute(&pool).await.unwrap();

        let client = test_helpers::TestClient::new(pool.clone());
        let diff_resp = client.post_json(
            "/v1/services/svc-wh/diffs",
            &serde_json::json!({
                "service_name": "WH Svc",
                "repo_url": "",
                "owner_team": "team",
                "from_git_ref": "v1",
                "to_git_ref": "v2",
                "spec_format": "openapi",
                "changes": []
            }),
        ).await;
        assert_eq!(diff_resp.status(), StatusCode::CREATED,
                   "diff creation failed: {}", diff_resp.text());

        // Dispatch is in a spawned task — wait up to 3 s for the delivery.
        echo.wait_for_requests(1, 3000).await;

        let reqs = echo.requests.lock().await;
        assert_eq!(reqs.len(), 1, "expected exactly one delivery, got {}", reqs.len());
        let r = &reqs[0];
        assert_eq!(r.method, "POST");
        assert!(r.path.starts_with("/hook"));
        // Payload must include diff_id and breaking_count.
        let payload: serde_json::Value = serde_json::from_str(&r.body).expect("delivery body must be JSON");
        assert!(payload["diff_id"].is_string(), "payload must contain diff_id");
        // HMAC signature header must be present.
        let has_sig = r.headers.iter().any(|(k, _)| k.to_lowercase() == "x-radar-signature-256");
        assert!(has_sig, "delivery must carry X-Radar-Signature-256 header");
    }

    // --- scheduled-scan happy-path ---

    #[tokio::test]
    async fn test_scan_create_valid_returns_201() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "svc-happy",
                    "spec_url": "https://93.184.216.34/openapi.yaml",
                    "interval_minutes": 30
                }),
            )
            .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = resp.json();
        assert!(body["id"].as_str().is_some());
        assert_eq!(body["service_id"], "svc-happy");
    }

    #[tokio::test]
    async fn test_scan_create_duplicate_returns_200() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let payload = serde_json::json!({
            "service_id": "svc-dupe",
            "spec_url": "https://93.184.216.34/openapi.yaml",
            "interval_minutes": 30
        });
        let r1 = client.post_json("/v1/scheduled-scans", &payload).await;
        assert_eq!(r1.status(), StatusCode::CREATED);
        let r2 = client.post_json("/v1/scheduled-scans", &payload).await;
        assert_eq!(r2.status(), StatusCode::OK);
        assert_eq!(r1.json()["id"], r2.json()["id"]);
    }

    #[tokio::test]
    async fn test_scan_list_shows_created_scan() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "svc-list",
                    "spec_url": "https://93.184.216.34/openapi.yaml",
                    "interval_minutes": 60
                }),
            )
            .await;
        let resp = client.get("/v1/scheduled-scans").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let arr = resp.json();
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["service_id"], "svc-list");
    }

    #[tokio::test]
    async fn test_scan_delete_existing_returns_204() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let created = client
            .post_json(
                "/v1/scheduled-scans",
                &serde_json::json!({
                    "service_id": "svc-del",
                    "spec_url": "https://93.184.216.34/openapi.yaml",
                    "interval_minutes": 15
                }),
            )
            .await;
        assert_eq!(created.status(), StatusCode::CREATED);
        let id = created.json()["id"].as_str().unwrap().to_string();
        let del = client.delete(&format!("/v1/scheduled-scans/{id}")).await;
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
        let list = client.get("/v1/scheduled-scans").await;
        assert_eq!(list.json().as_array().unwrap().len(), 0);
    }

    // -----------------------------------------------------------------------
    // EPIC K — Digest Preview (TD-NEW-1)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_digest_preview_returns_html_document() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client
            .post_json("/v1/notifications/digest/preview", &serde_json::json!({}))
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.text();
        assert!(body.contains("<!DOCTYPE html>"), "response must be an HTML document");
        assert!(body.contains("API Radar"), "response must contain product branding");
        assert!(body.contains("Weekly Digest"), "response must contain digest heading");
    }

    // -----------------------------------------------------------------------
    // Phase 2 — GET /v1/readiness
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_readiness_returns_setup_required_on_empty_db() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);
        let resp = client.get("/v1/readiness").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_str(resp.text()).unwrap();
        assert_eq!(body["overall"], "setup_required");
        let items = body["items"].as_array().unwrap();
        // db_connected is always ok
        let db_item = items.iter().find(|i| i["name"] == "db_connected").unwrap();
        assert_eq!(db_item["status"], "ok");
        // service_registered is missing on empty DB
        let svc_item = items.iter().find(|i| i["name"] == "service_registered").unwrap();
        assert_eq!(svc_item["status"], "missing");
    }

    #[tokio::test]
    async fn test_readiness_returns_ready_after_service_diff_consumer() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);

        // Register a service + diff
        let svc = client.post_json(
            "/v1/services",
            &serde_json::json!({ "name": "svc-a", "repo_url": "https://github.com/x/y", "owner_team": "eng", "spec_format": "openapi" }),
        ).await;
        assert_eq!(svc.status(), StatusCode::CREATED);
        let svc_body: Value = serde_json::from_str(svc.text()).unwrap();
        let svc_id = svc_body["id"].as_str().unwrap();

        let diff = client.post_json(
            &format!("/v1/services/{svc_id}/diffs"),
            &serde_json::json!({ "service_name": "svc-a", "repo_url": "https://github.com/x/y", "owner_team": "eng", "from_git_ref": "abc", "to_git_ref": "def", "spec_format": "openapi", "changes": [] }),
        ).await;
        assert_eq!(diff.status(), StatusCode::CREATED);

        // Register a consumer
        let con = client.post_json(
            "/v1/consumers",
            &serde_json::json!({ "name": "con-a", "repo_url": "https://github.com/x/z", "owner_team": "eng", "contact": "a@b.com" }),
        ).await;
        assert_eq!(con.status(), StatusCode::CREATED);

        let resp = client.get("/v1/readiness").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_str(resp.text()).unwrap();
        assert_eq!(body["overall"], "ready");
    }

    // -----------------------------------------------------------------------
    // Diffs — pagination
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_all_diffs_per_page_is_respected() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);

        let svc = client.post_json(
            "/v1/services",
            &serde_json::json!({ "name": "pag-svc", "repo_url": "", "owner_team": "", "spec_format": "openapi" }),
        ).await;
        let svc_id = serde_json::from_str::<Value>(svc.text()).unwrap()["id"]
            .as_str().unwrap().to_string();

        for i in 0..3u32 {
            client.post_json(
                &format!("/v1/services/{svc_id}/diffs"),
                &serde_json::json!({ "service_name": "pag-svc", "repo_url": "", "owner_team": "", "from_git_ref": format!("a{i}"), "to_git_ref": format!("b{i}"), "spec_format": "openapi", "changes": [] }),
            ).await;
        }

        let resp = client.get("/v1/diffs?limit=2").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_str(resp.text()).unwrap();
        assert!(body.as_array().unwrap().len() <= 2, "limit=2 must return at most 2 diffs");
    }

    // -----------------------------------------------------------------------
    // Diffs — org isolation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_diffs_are_scoped_to_org() {
        let pool = test_pool().await;
        let client_a = test_helpers::TestClient::new_with_jwt(pool.clone(), "org-alpha");
        let client_b = test_helpers::TestClient::new_with_jwt(pool.clone(), "org-beta");

        // Register service and diff under org-alpha
        let svc = client_a.post_json(
            "/v1/services",
            &serde_json::json!({ "name": "iso-svc", "repo_url": "", "owner_team": "", "spec_format": "openapi" }),
        ).await;
        assert_eq!(svc.status(), StatusCode::CREATED);
        let svc_id = serde_json::from_str::<Value>(svc.text()).unwrap()["id"]
            .as_str().unwrap().to_string();

        client_a.post_json(
            &format!("/v1/services/{svc_id}/diffs"),
            &serde_json::json!({ "service_name": "iso-svc", "repo_url": "", "owner_team": "", "from_git_ref": "x", "to_git_ref": "y", "spec_format": "openapi", "changes": [] }),
        ).await;

        // org-beta must not see org-alpha's diffs in the global list
        let resp = client_b.get("/v1/diffs").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let diffs: Vec<Value> = serde_json::from_str(resp.text()).unwrap();
        let found = diffs.iter().any(|d| d["service_id"].as_str() == Some(&svc_id));
        assert!(!found, "org-beta must not see org-alpha's diffs");
    }

    // -----------------------------------------------------------------------
    // Consumers — upsert idempotency
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_upsert_consumer_by_name_is_idempotent() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);

        let first = client.post_json(
            "/v1/consumers/upsert",
            &serde_json::json!({ "name": "upsert-consumer", "catalog_source": "test" }),
        ).await;
        assert_eq!(first.status(), StatusCode::CREATED);
        let id1 = serde_json::from_str::<Value>(first.text()).unwrap()["id"]
            .as_str().unwrap().to_string();

        let second = client.post_json(
            "/v1/consumers/upsert",
            &serde_json::json!({ "name": "upsert-consumer", "catalog_source": "test" }),
        ).await;
        assert_eq!(second.status(), StatusCode::OK);
        let id2 = serde_json::from_str::<Value>(second.text()).unwrap()["id"]
            .as_str().unwrap().to_string();

        assert_eq!(id1, id2, "upsert with same name must return the same consumer id");
    }

    // -----------------------------------------------------------------------
    // Services — duplicate names allowed (uniqueness is on id, not name)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_service_same_name_produces_distinct_ids() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);

        let body = serde_json::json!({ "name": "dup-svc", "repo_url": "", "owner_team": "", "spec_format": "openapi" });
        let first = client.post_json("/v1/services", &body).await;
        assert_eq!(first.status(), StatusCode::CREATED);
        let id1 = serde_json::from_str::<Value>(first.text()).unwrap()["id"]
            .as_str().unwrap().to_string();

        let second = client.post_json("/v1/services", &body).await;
        assert_eq!(second.status(), StatusCode::CREATED,
            "same-name services are allowed; uniqueness is enforced on id only");
        let id2 = serde_json::from_str::<Value>(second.text()).unwrap()["id"]
            .as_str().unwrap().to_string();

        assert_ne!(id1, id2, "two services with the same name must get distinct ids");
    }

    // -----------------------------------------------------------------------
    // Audit events — org scoping and secret redaction
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_audit_event_secret_fields_are_redacted() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);

        let post = client.post_json(
            "/v1/audit-events",
            &serde_json::json!({
                "actor": "ci-bot",
                "action": "secret.redact.test",
                "meta": { "api_key": "super-secret-key", "label": "visible" }
            }),
        ).await;
        assert_eq!(post.status(), StatusCode::CREATED);

        // POST returns {"ok": true}; retrieve the stored event via the list endpoint.
        let list = client.get("/v1/audit-events?action=secret.redact.test").await;
        assert_eq!(list.status(), StatusCode::OK);
        let body: Value = serde_json::from_str(list.text()).unwrap();
        let entry = &body["entries"][0];
        let meta = &entry["meta"];
        assert_ne!(meta["api_key"], "super-secret-key",
            "api_key must be redacted in stored audit event");
        assert_eq!(meta["label"], "visible", "non-secret field must be preserved");
    }

    // -----------------------------------------------------------------------
    // Evidence coverage — returns flat rows
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_evidence_coverage_returns_array() {
        let pool = test_pool().await;
        let client = test_helpers::TestClient::new(pool);

        let resp = client.get("/v1/evidence/coverage").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_str(resp.text()).unwrap();
        assert!(body.is_array(), "evidence/coverage must return a JSON array");
    }
}

