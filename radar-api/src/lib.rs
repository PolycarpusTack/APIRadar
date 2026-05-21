use anyhow::Result;
use axum::{
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::any::AnyPoolOptions;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_http::{cors::{AllowOrigin, Any, CorsLayer}, services::ServeDir, timeout::TimeoutLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// D-7: Per-IP sliding-window rate limiter
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RateLimiter {
    // key (client IP or "unknown") → (request count, window start)
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

    /// Returns `true` if the request is allowed; `false` if the limit is exceeded.
    fn check_and_record(&self, key: &str) -> bool {
        if self.max_per_minute == 0 {
            return true; // unlimited
        }
        let mut state = self.state.lock().unwrap();
        let now = std::time::Instant::now();
        let entry = state.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) >= std::time::Duration::from_secs(60) {
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

fn client_key(req: &Request) -> String {
    req.headers()
        .get("x-forwarded-for")
        .or_else(|| req.headers().get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// D-4: JWT claims (HS256) for org-scoped tokens
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct JwtClaims {
    pub sub: String,
    pub org_id: String,
    pub exp: usize,
}

/// Validate an HS256 JWT using RADAR_JWT_SECRET. Returns claims on success.
fn validate_jwt(token: &str, secret: &str) -> Option<JwtClaims> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    decode::<JwtClaims>(token, &key, &validation)
        .ok()
        .map(|d| d.claims)
}

// ---------------------------------------------------------------------------
// D-4: OIDC authorization code flow
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct OidcConfig {
    provider_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    /// Which claim in the userinfo/ID-token to use as `org_id`.
    /// Defaults to "hd" (Google Workspace hosted domain). Falls back to "sub".
    org_claim: String,
}

impl OidcConfig {
    fn from_env() -> Option<Self> {
        let provider_url = std::env::var("RADAR_OIDC_PROVIDER_URL").ok()?;
        let client_id = std::env::var("RADAR_OIDC_CLIENT_ID").ok()?;
        let client_secret = std::env::var("RADAR_OIDC_CLIENT_SECRET").ok()?;
        let redirect_uri = std::env::var("RADAR_OIDC_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:8080/auth/callback".to_string());
        let org_claim = std::env::var("RADAR_OIDC_ORG_CLAIM")
            .unwrap_or_else(|_| "hd".to_string());
        Some(OidcConfig { provider_url, client_id, client_secret, redirect_uri, org_claim })
    }
}

#[derive(serde::Deserialize)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    userinfo_endpoint: Option<String>,
}

#[derive(serde::Deserialize)]
struct OidcTokenResponse {
    access_token: String,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct OidcUserInfo {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    hd: Option<String>,
    /// Display name from the identity provider — captured for future use.
    #[allow(dead_code)]
    #[serde(default)]
    name: Option<String>,
}

/// Short-lived CSRF state token embedded as a signed JWT.
#[derive(serde::Serialize, serde::Deserialize)]
struct OidcState {
    nonce: String,
    exp: usize,
}

/// Sign a JwtClaims struct into an HS256 JWT string.
fn sign_jwt(claims: &JwtClaims, secret: &str) -> Option<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    encode(&Header::new(Algorithm::HS256), claims, &EncodingKey::from_secret(secret.as_bytes())).ok()
}

/// Sign an OidcState into an HS256 JWT string.
fn sign_state(state: &OidcState, secret: &str) -> Option<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    encode(&Header::new(Algorithm::HS256), state, &EncodingKey::from_secret(secret.as_bytes())).ok()
}

/// Validate an OidcState JWT and return the nonce if valid.
fn validate_state(token: &str, secret: &str) -> Option<String> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut v = Validation::new(Algorithm::HS256);
    v.validate_exp = true;
    decode::<OidcState>(token, &key, &v).ok().map(|d| d.claims.nonce)
}

async fn fetch_discovery(provider_url: &str) -> anyhow::Result<OidcDiscovery> {
    let url = format!("{provider_url}/.well-known/openid-configuration");
    let disc: OidcDiscovery = reqwest::get(&url).await?.json().await?;
    Ok(disc)
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    header.split(';').find_map(|part| {
        part.trim().strip_prefix(&prefix).map(|v| v.trim().to_string())
    })
}

fn urlencoding_encode(s: &str) -> String {
    s.chars().flat_map(|c| {
        if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            vec![c]
        } else {
            format!("%{:02X}", c as u32).chars().collect()
        }
    }).collect()
}

fn base64_decode_url(s: &str) -> Option<Vec<u8>> {
    let standard = s.replace('-', "+").replace('_', "/");
    base64_decode_simple(&standard)
}

fn base64_decode_simple(s: &str) -> Option<Vec<u8>> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s = s.trim_end_matches('=');
    let mut result = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in s.as_bytes() {
        let val = CHARS.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(result)
}

/// GET /auth/login — redirect to OIDC provider authorization endpoint.
async fn oidc_login() -> Response {
    use axum::http::header::{LOCATION, SET_COOKIE};
    let Some(cfg) = OidcConfig::from_env() else {
        return (StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "OIDC not configured — set RADAR_OIDC_PROVIDER_URL, RADAR_OIDC_CLIENT_ID, RADAR_OIDC_CLIENT_SECRET"}))).into_response();
    };
    let jwt_secret = std::env::var("RADAR_JWT_SECRET").unwrap_or_else(|_| "oidc-state-key".to_string());
    let disc = match fetch_discovery(&cfg.provider_url).await {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("OIDC discovery failed: {e}")}))).into_response(),
    };
    let nonce = Uuid::new_v4().to_string();
    let state_claims = OidcState {
        nonce: nonce.clone(),
        exp: (Utc::now() + Duration::minutes(10)).timestamp() as usize,
    };
    let state_token = match sign_state(&state_claims, &jwt_secret) {
        Some(t) => t,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "state signing failed"}))).into_response(),
    };
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid+email+profile&state={}",
        disc.authorization_endpoint,
        urlencoding_encode(&cfg.client_id),
        urlencoding_encode(&cfg.redirect_uri),
        urlencoding_encode(&state_token),
    );
    let state_cookie = format!(
        "oidc_state={state_token}; HttpOnly; SameSite=Lax; Max-Age=600; Path=/"
    );
    (
        StatusCode::FOUND,
        [(LOCATION, auth_url), (SET_COOKIE, state_cookie)],
    ).into_response()
}

/// GET /auth/callback?code=...&state=... — exchange code, issue session cookie.
async fn oidc_callback(Query(params): Query<HashMap<String, String>>, req: Request) -> Response {
    use axum::http::header::{LOCATION, SET_COOKIE};
    let Some(cfg) = OidcConfig::from_env() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "OIDC not configured"}))).into_response();
    };
    let jwt_secret = std::env::var("RADAR_JWT_SECRET").unwrap_or_else(|_| "oidc-state-key".to_string());

    // Verify CSRF state
    let state_param = params.get("state").cloned().unwrap_or_default();
    let cookie_header = req.headers().get("cookie").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let state_cookie_val = parse_cookie(&cookie_header, "oidc_state");
    if state_cookie_val.as_deref() != Some(state_param.as_str()) || validate_state(&state_param, &jwt_secret).is_none() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid or expired state"}))).into_response();
    }

    let code = match params.get("code") {
        Some(c) => c.clone(),
        None => return (StatusCode::BAD_REQUEST, Json(json!({"error": "missing code"}))).into_response(),
    };

    let disc = match fetch_discovery(&cfg.provider_url).await {
        Ok(d) => d,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("OIDC discovery failed: {e}")}))).into_response(),
    };

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let token_resp = client
        .post(&disc.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &cfg.redirect_uri),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
        ])
        .send()
        .await;
    let token_resp: OidcTokenResponse = match token_resp {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(t) => t,
            Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("token parse failed: {e}")}))).into_response(),
        },
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            return (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("token endpoint {status}: {body}")}))).into_response();
        }
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({"error": format!("token request failed: {e}")}))).into_response(),
    };

    // Fetch user info
    let userinfo_url = disc.userinfo_endpoint.as_deref().unwrap_or("").to_string();
    let userinfo: OidcUserInfo = if !userinfo_url.is_empty() {
        match client
            .get(&userinfo_url)
            .bearer_auth(&token_resp.access_token)
            .send()
            .await
        {
            Ok(r) => r.json().await.unwrap_or_default(),
            Err(_) => OidcUserInfo::default(),
        }
    } else {
        // Try to decode claims from id_token JWT payload (middle segment).
        // We only extract claims from the payload — no signature verification needed here
        // since the session JWT we issue is what we sign and verify for auth.
        token_resp.id_token.as_deref()
            .and_then(|t| t.split('.').nth(1))
            .and_then(|b| {
                let padded = format!("{b}{}", "=".repeat((4 - b.len() % 4) % 4));
                base64_decode_url(&padded)
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            })
            .unwrap_or_default()
    };

    // Derive org_id from configured claim
    let org_id = if cfg.org_claim == "hd" {
        userinfo.hd.clone().unwrap_or_else(|| userinfo.sub.clone())
    } else {
        userinfo.sub.clone()
    };

    let sub = userinfo.email.as_deref().unwrap_or(&userinfo.sub).to_string();
    let session_claims = JwtClaims {
        sub,
        org_id,
        exp: (Utc::now() + Duration::hours(24)).timestamp() as usize,
    };
    let session_token = match sign_jwt(&session_claims, &jwt_secret) {
        Some(t) => t,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "session signing failed"}))).into_response(),
    };

    let secure_flag = if cfg.redirect_uri.starts_with("https") { "; Secure" } else { "" };
    let session_cookie = format!(
        "radar_session={session_token}; HttpOnly; SameSite=Lax; Max-Age=86400; Path=/{secure_flag}"
    );
    let clear_state = "oidc_state=; HttpOnly; SameSite=Lax; Max-Age=0; Path=/".to_string();

    // Two SET_COOKIE headers require a manually built HeaderMap since Axum tuples
    // deduplicate keys.
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(LOCATION, "/app/".parse().unwrap());
    headers.append(SET_COOKIE, session_cookie.parse().unwrap());
    headers.append(SET_COOKIE, clear_state.parse().unwrap());

    (StatusCode::FOUND, headers).into_response()
}

/// GET /auth/me — return current session claims (JSON).
async fn oidc_me(req: Request) -> Response {
    let jwt_secret = req
        .extensions()
        .get::<JwtSecretExt>()
        .and_then(|s| s.0.clone())
        .or_else(|| std::env::var("RADAR_JWT_SECRET").ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    if jwt_secret.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "auth not configured"}))).into_response();
    }
    let cookie_header = req.headers().get("cookie").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let token = parse_cookie(&cookie_header, "radar_session");
    match token.and_then(|t| validate_jwt(&t, &jwt_secret)) {
        Some(claims) => Json(json!({"sub": claims.sub, "org_id": claims.org_id})).into_response(),
        None => (StatusCode::UNAUTHORIZED, Json(json!({"error": "not authenticated"}))).into_response(),
    }
}

/// GET /auth/logout — clear session cookie, redirect to /app/login.
async fn oidc_logout() -> Response {
    use axum::http::header::{LOCATION, SET_COOKIE};
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(LOCATION, "/app/login".parse().unwrap());
    headers.insert(SET_COOKIE, "radar_session=; HttpOnly; SameSite=Lax; Max-Age=0; Path=/".parse().unwrap());
    (StatusCode::FOUND, headers).into_response()
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

enum ApiError {
    Db(sqlx::Error),
    BadRequest(String),
    NotFound(String),
    Unauthorized,
    TooManyRequests(String),
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Db(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Db(e) => {
                tracing::error!("database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "internal server error"})),
                )
                    .into_response()
            }
            ApiError::BadRequest(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": msg})),
            )
                .into_response(),
            ApiError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": msg})),
            )
                .into_response(),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response(),
            ApiError::TooManyRequests(msg) => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({"error": msg})),
            )
                .into_response(),
        }
    }
}

// ---------------------------------------------------------------------------
// Prometheus metrics handle (initialized once, shared via OnceLock)
// ---------------------------------------------------------------------------

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static PROMETHEUS: OnceLock<PrometheusHandle> = OnceLock::new();

fn get_prometheus_handle() -> &'static PrometheusHandle {
    PROMETHEUS.get_or_init(|| {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder).ok();
        handle
    })
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

    let mut res = next.run(req).await;
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

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(effective_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    info!("migrations applied");

    let require_auth = std::env::var("RADAR_REQUIRE_AUTH")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let jwt_secret = std::env::var("RADAR_JWT_SECRET").ok().filter(|s| !s.is_empty());

    let limiter = Arc::new(RateLimiter::new(rate_limit_per_minute));
    let app = build_router(pool, static_dir, max_body_bytes, require_auth, jwt_secret);

    // D-7: Add rate limiting as the outermost layer so it wraps the entire app.
    let app = app.layer(middleware::from_fn(move |req: Request, next: Next| {
        let lim = limiter.clone();
        async move {
            let key = client_key(&req);
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

    axum::serve(listener, app).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

/// Captured at router-build time so tests can't contaminate each other via env vars.
#[derive(Clone, Copy)]
struct RequireAuth(bool);

/// JWT secret injected at build time; falls back to RADAR_JWT_SECRET env var at runtime.
#[derive(Clone)]
struct JwtSecretExt(Option<String>);

async fn auth_middleware(
    State(pool): State<sqlx::AnyPool>,
    mut req: Request,
    next: Next,
) -> Response {
    // This middleware is scoped to the /v1 sub-router; /health and /metrics are
    // on the outer router and never reach here.
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // D-4: JWT validation — prefer build-time secret (test-safe), fall back to env var.
    let jwt_secret = req
        .extensions()
        .get::<JwtSecretExt>()
        .and_then(|s| s.0.clone())
        .or_else(|| std::env::var("RADAR_JWT_SECRET").ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    if !jwt_secret.is_empty() {
        // D-4: Also accept session cookie as auth (set by OIDC callback).
        let cookie_header_str = req
            .headers()
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bearer = auth_header.strip_prefix("Bearer ").unwrap_or("");
        if let Some(claims) = validate_jwt(bearer, &jwt_secret) {
            // Inject org_id into request extensions for downstream handlers.
            req.extensions_mut().insert(claims);
            return next.run(req).await;
        }

        // Check cookie as fallback for dashboard sessions.
        if let Some(session_tok) = parse_cookie(&cookie_header_str, "radar_session") {
            if let Some(claims) = validate_jwt(&session_tok, &jwt_secret) {
                req.extensions_mut().insert(claims);
                return next.run(req).await;
            }
        }

        drop(pool);
        return ApiError::Unauthorized.into_response();
    }

    // Legacy static token auth (backwards-compatible when RADAR_JWT_SECRET is not set).
    let service_token = std::env::var("RADAR_SERVICE_TOKEN").unwrap_or_default();
    if service_token.is_empty() {
        // require_auth is set at build time (see build_router) to avoid request-time env reads.
        let require_auth = req.extensions().get::<RequireAuth>().map(|r| r.0).unwrap_or(false);
        if require_auth {
            drop(pool);
            return ApiError::Unauthorized.into_response();
        }
        return next.run(req).await;
    }

    let expected = format!("Bearer {service_token}");
    if auth_header != expected {
        drop(pool);
        return ApiError::Unauthorized.into_response();
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router(pool: sqlx::AnyPool, static_dir: Option<&str>, max_body_bytes: usize, require_auth: bool, jwt_secret: Option<String>) -> Router {

    let v1 = Router::new()
        .route("/services", get(list_services).post(create_service))
        .route("/services/:id", get(get_service))
        .route("/services/:id/diffs", get(list_diffs).post(create_diff))
        .route("/services/:id/consumers", get(list_consumers))
        .route("/services/:id/subscriptions", post(create_subscription))
        .route("/consumers", get(list_all_consumers).post(create_consumer))
        .route("/diffs", get(list_all_diffs))
        .route("/diffs/:id", get(get_diff))
        .route("/diffs/:id/blast-radius", get(blast_radius))
        .route("/usage/events", post(ingest_usage_event))
        .route("/call-sites", post(upsert_call_sites))
        .route("/summary", get(get_summary))
        .route("/generate-tests", post(generate_tests))
        .route("/generate-tests", get(list_test_suites))
        .route("/generate-tests/:id", get(get_test_suite))
        .route("/sandbox-envs", get(list_sandbox_envs).post(create_sandbox_env))
        .route("/sandbox-envs/:id", axum::routing::put(update_sandbox_env).delete(delete_sandbox_env))
        .route("/spec-versions", get(list_spec_versions))
        .route("/spec-versions/:id/raw", get(get_spec_version_raw))
        .route("/settings", get(get_settings).put(update_settings))
        .route("/settings/integrations", get(get_integrations))
        .route("/release-notes", get(list_release_notes))
        .route("/release-notes/:id", get(get_release_note))
        .route("/diffs/:id/release-notes", post(create_release_note))
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
        .route("/auth/login", get(oidc_login))
        .route("/auth/callback", get(oidc_callback))
        .route("/auth/me", get(oidc_me))
        .route("/auth/logout", get(oidc_logout))
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
struct CreateDiffBody {
    service_name: String,
    repo_url: String,
    owner_team: String,
    from_git_ref: String,
    to_git_ref: String,
    pr_url: Option<String>,
    spec_format: String,
    spec_yaml: Option<String>,
    #[serde(default)]
    changes: Vec<ChangeInput>,
}

#[derive(Deserialize)]
struct ChangeInput {
    path: String,
    kind: String,
    severity: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct PaginationParams {
    #[serde(default = "default_page_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_page_limit() -> i64 {
    50
}

#[derive(Deserialize)]
struct CreateServiceBody {
    id: Option<String>,
    name: String,
    repo_url: String,
    owner_team: String,
    spec_format: String,
}

#[derive(Deserialize)]
struct CreateConsumerBody {
    name: String,
    repo_url: String,
    owner_team: String,
    contact: String,
}

#[derive(Deserialize)]
struct CreateSubscriptionBody {
    consumer_id: String,
}

#[derive(Deserialize)]
struct UsageEventRequest {
    consumer_id: String,
    service_id: String,
    operation: String,
    #[serde(default)]
    field_path: String,
}

#[derive(Deserialize)]
struct CallSiteInput {
    consumer_id: String,
    service_id: String,
    #[serde(default)]
    operation: String,
    file_path: String,
    line_number: i64,
    field_path: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deterministic UUID v5 for a spec_version identified by (service_id, git_ref).
fn spec_version_id(service_id: &str, git_ref: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{service_id}:{git_ref}").as_bytes(),
    )
    .to_string()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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
async fn metrics_handler() -> impl IntoResponse {
    let body = get_prometheus_handle().render();
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

// GET /v1/services/:id/diffs
async fn list_diffs(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    // Fetch diffs for this service joined through spec_version.
    let rows = sqlx::query(
        r#"
        SELECT
            d.id          AS diff_id,
            sv_from.git_ref AS from_git_ref,
            sv_to.git_ref   AS to_git_ref,
            d.pr_url,
            d.created_at,
            (
                SELECT COUNT(*)
                FROM change c
                WHERE c.diff_id = d.id
                  AND c.severity = 'breaking'
            ) AS breaking_count
        FROM diff d
        JOIN spec_version sv_from ON sv_from.id = d.from_version
        JOIN spec_version sv_to   ON sv_to.id   = d.to_version
        WHERE sv_from.service_id = ?
           OR sv_to.service_id   = ?
        ORDER BY d.created_at DESC
        "#,
    )
    .bind(&service_id)
    .bind(&service_id)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            let breaking_count: i64 = row.try_get("breaking_count").unwrap_or(0);
            json!({
                "id":             row.get::<String, _>("diff_id"),
                "from_git_ref":   row.get::<String, _>("from_git_ref"),
                "to_git_ref":     row.get::<String, _>("to_git_ref"),
                "pr_url":         row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
                "created_at":     row.get::<String, _>("created_at"),
                "breaking_count": breaking_count,
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// POST /v1/services/:id/diffs
async fn create_diff(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<CreateDiffBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.from_git_ref.is_empty() {
        return Err(ApiError::BadRequest("from_git_ref is required".into()));
    }
    if body.to_git_ref.is_empty() {
        return Err(ApiError::BadRequest("to_git_ref is required".into()));
    }

    let now = Utc::now().to_rfc3339();

    // 1. Upsert service row.
    sqlx::query(
        r#"
        INSERT INTO service (id, name, repo_url, owner_team, spec_format)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name       = excluded.name,
            repo_url   = excluded.repo_url,
            owner_team = excluded.owner_team,
            spec_format = excluded.spec_format
        "#,
    )
    .bind(&service_id)
    .bind(&body.service_name)
    .bind(&body.repo_url)
    .bind(&body.owner_team)
    .bind(&body.spec_format)
    .execute(&pool)
    .await?;

    // 2. Upsert from_version spec_version row (no spec_yaml — the base spec is historical).
    let from_version_id = spec_version_id(&service_id, &body.from_git_ref);
    sqlx::query(
        r#"
        INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(&from_version_id)
    .bind(&service_id)
    .bind(&body.from_git_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .execute(&pool)
    .await?;

    // 3. Upsert to_version spec_version row, storing spec_yaml if provided.
    let to_version_id = spec_version_id(&service_id, &body.to_git_ref);
    sqlx::query(
        r#"
        INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            spec_yaml = COALESCE(excluded.spec_yaml, spec_version.spec_yaml)
        "#,
    )
    .bind(&to_version_id)
    .bind(&service_id)
    .bind(&body.to_git_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .bind(&body.spec_yaml)
    .execute(&pool)
    .await?;

    // 4. Deduplication: return cached diff if this exact transition already exists.
    {
        use sqlx::Row;
        let existing = sqlx::query(
            "SELECT id FROM diff WHERE from_version = ? AND to_version = ?",
        )
        .bind(&from_version_id)
        .bind(&to_version_id)
        .fetch_optional(&pool)
        .await?;

        if let Some(row) = existing {
            let existing_id: String = row.try_get("id").map_err(ApiError::Db)?;
            return Ok((
                StatusCode::OK,
                Json(json!({
                    "id":           existing_id,
                    "from_version": from_version_id,
                    "to_version":   to_version_id,
                    "cached":       true,
                })),
            ));
        }
    }

    // 5. Insert diff row.
    let diff_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO diff (id, from_version, to_version, pr_url, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&diff_id)
    .bind(&from_version_id)
    .bind(&to_version_id)
    .bind(&body.pr_url)
    .bind(&now)
    .execute(&pool)
    .await?;

    // 6. Insert change rows.
    for change in &body.changes {
        let change_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO change (id, diff_id, path, kind, severity, description)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&change_id)
        .bind(&diff_id)
        .bind(&change.path)
        .bind(&change.kind)
        .bind(&change.severity)
        .bind(&change.description)
        .execute(&pool)
        .await?;
    }

    metrics::counter!("radar_diffs_created_total").increment(1);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":           diff_id,
            "from_version": from_version_id,
            "to_version":   to_version_id,
            "created_at":   now,
        })),
    ))
}

// GET /v1/services/:id/consumers
async fn list_consumers(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"
        SELECT c.id, c.name, c.repo_url, c.owner_team, c.contact
        FROM consumer c
        JOIN subscription s ON s.consumer_id = c.id
        WHERE s.service_id = ?
        "#,
    )
    .bind(&service_id)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":         row.get::<String, _>("id"),
                "name":       row.get::<String, _>("name"),
                "repo_url":   row.get::<String, _>("repo_url"),
                "owner_team": row.get::<String, _>("owner_team"),
                "contact":    row.get::<String, _>("contact"),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// POST /v1/consumers
async fn create_consumer(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateConsumerBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if body.repo_url.trim().is_empty() {
        return Err(ApiError::BadRequest("repo_url must not be empty".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO consumer (id, name, repo_url, owner_team, contact, org_id)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(&id)
    .bind(&body.name)
    .bind(&body.repo_url)
    .bind(&body.owner_team)
    .bind(&body.contact)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    metrics::counter!("radar_consumers_created_total").increment(1);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":   id,
            "name": body.name,
        })),
    ))
}

// POST /v1/services/:id/subscriptions
async fn create_subscription(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<CreateSubscriptionBody>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    // Verify consumer exists.
    let consumer_exists = sqlx::query("SELECT id FROM consumer WHERE id = ?")
        .bind(&body.consumer_id)
        .fetch_optional(&pool)
        .await?;
    if consumer_exists.is_none() {
        return Err(ApiError::NotFound(format!(
            "consumer {} not found",
            body.consumer_id
        )));
    }

    // Verify service exists.
    let service_exists = sqlx::query("SELECT id FROM service WHERE id = ?")
        .bind(&service_id)
        .fetch_optional(&pool)
        .await?;
    if service_exists.is_none() {
        return Err(ApiError::NotFound(format!(
            "service {service_id} not found"
        )));
    }

    // Check for existing subscription — idempotent.
    let existing = sqlx::query(
        "SELECT id, service_id, consumer_id, opted_in_at FROM subscription WHERE service_id = ? AND consumer_id = ?",
    )
    .bind(&service_id)
    .bind(&body.consumer_id)
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        let resp = json!({
            "id":          row.get::<String, _>("id"),
            "service_id":  row.get::<String, _>("service_id"),
            "consumer_id": row.get::<String, _>("consumer_id"),
            "opted_in_at": row.get::<String, _>("opted_in_at"),
        });
        return Ok((StatusCode::OK, Json(resp)));
    }

    let sub_id = Uuid::new_v4().to_string();
    let opted_in_at = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO subscription (id, service_id, consumer_id, opted_in_at)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(&sub_id)
    .bind(&service_id)
    .bind(&body.consumer_id)
    .bind(&opted_in_at)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":          sub_id,
            "service_id":  service_id,
            "consumer_id": body.consumer_id,
            "opted_in_at": opted_in_at,
        })),
    ))
}

// GET /v1/diffs/:id
async fn get_diff(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"
        SELECT d.id, sv_from.git_ref AS from_git_ref, sv_to.git_ref AS to_git_ref,
               d.pr_url, d.created_at, sv_to.spec_yaml
        FROM diff d
        JOIN spec_version sv_from ON sv_from.id = d.from_version
        JOIN spec_version sv_to   ON sv_to.id   = d.to_version
        WHERE d.id = ?
        "#,
    )
    .bind(&diff_id)
    .fetch_optional(&pool)
    .await?;

    let row = match row {
        None => return Err(ApiError::NotFound(format!("diff {diff_id} not found"))),
        Some(r) => r,
    };

    // Fetch associated change rows.
    let change_rows = sqlx::query(
        r#"
        SELECT path, kind, severity, description
        FROM change
        WHERE diff_id = ?
        ORDER BY path, kind
        "#,
    )
    .bind(&diff_id)
    .fetch_all(&pool)
    .await?;

    let changes: Vec<Value> = change_rows
        .iter()
        .map(|c| {
            json!({
                "path":        c.get::<String, _>("path"),
                "kind":        c.get::<String, _>("kind"),
                "severity":    c.get::<String, _>("severity"),
                "description": c.try_get::<Option<String>, _>("description").unwrap_or(None),
            })
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(json!({
            "id":           row.get::<String, _>("id"),
            "from_git_ref": row.get::<String, _>("from_git_ref"),
            "to_git_ref":   row.get::<String, _>("to_git_ref"),
            "pr_url":       row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
            "created_at":   row.get::<String, _>("created_at"),
            "spec_yaml":    row.try_get::<Option<String>, _>("spec_yaml").unwrap_or(None),
            "changes":      changes,
        })),
    ))
}

// GET /v1/diffs/:id/blast-radius
async fn blast_radius(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    // 1. Verify diff exists.
    let diff_row = sqlx::query("SELECT id, from_version, to_version FROM diff WHERE id = ?")
        .bind(&diff_id)
        .fetch_optional(&pool)
        .await?;

    let diff_row = match diff_row {
        Some(r) => r,
        None => return Err(ApiError::NotFound(format!("diff {diff_id} not found"))),
    };

    let to_version: String = diff_row.try_get("to_version").map_err(ApiError::Db)?;

    // 2. Get service_id from spec_version.
    let sv_row = sqlx::query("SELECT service_id FROM spec_version WHERE id = ?")
        .bind(&to_version)
        .fetch_optional(&pool)
        .await?;

    let service_id: String = match sv_row {
        Some(r) => r.try_get("service_id").map_err(ApiError::Db)?,
        None => {
            return Ok((
                StatusCode::OK,
                Json(json!({
                    "diff_id": diff_id,
                    "service_id": "",
                    "lookback_days": 30,
                    "entries": [],
                })),
            ))
        }
    };

    // 3. Fetch all changes for this diff to get changed paths.
    let change_rows = sqlx::query("SELECT path FROM change WHERE diff_id = ?")
        .bind(&diff_id)
        .fetch_all(&pool)
        .await?;

    // Parse each change path into op-level changes and (operation, field) pairs.
    // Path format: "GET /users" (op-level) or "GET /users → response.phone" (field-level).
    // We separate these so usage_event queries can be precise: op-level changes match any
    // telemetry for that operation, field-level changes require (operation AND field_path).
    let mut op_level_ops: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut changed_fields: Vec<(String, String)> = Vec::new(); // (operation, field_path)

    for row in &change_rows {
        let path: String = row.try_get("path").map_err(ApiError::Db)?;
        if let Some(arrow_pos) = path.find(" \u{2192} ") {
            let op = path[..arrow_pos].to_string();
            let after_arrow = &path[arrow_pos + " → ".len()..];
            let field = if let Some(stripped) = after_arrow.strip_prefix("response.") {
                stripped.to_string()
            } else {
                after_arrow.to_string()
            };
            changed_fields.push((op, field));
        } else {
            op_level_ops.insert(path);
        }
    }

    // Field-level (op, field) pairs whose operation has no op-level change — need precise match.
    let field_level_only: Vec<(String, String)> = {
        let mut seen = std::collections::HashSet::new();
        changed_fields
            .iter()
            .filter(|(op, _)| !op_level_ops.contains(op.as_str()))
            .filter(|(op, fp)| seen.insert((op.clone(), fp.clone())))
            .cloned()
            .collect()
    };

    // 4. Fetch all subscribed consumers for this service.
    let consumer_rows = sqlx::query(
        r#"
        SELECT c.id, c.name, c.repo_url, c.owner_team, c.contact
        FROM consumer c
        JOIN subscription s ON s.consumer_id = c.id
        WHERE s.service_id = ?
        "#,
    )
    .bind(&service_id)
    .fetch_all(&pool)
    .await?;

    let lookback_days: i64 = 30;
    let cutoff_30 = (Utc::now() - Duration::days(lookback_days)).to_rfc3339();
    let cutoff_7 = (Utc::now() - Duration::days(7)).to_rfc3339();

    let mut entries: Vec<Value> = Vec::new();

    for consumer_row in &consumer_rows {
        let consumer_id: String = consumer_row.try_get("id").map_err(ApiError::Db)?;
        let consumer_name: String = consumer_row.try_get("name").map_err(ApiError::Db)?;
        let consumer_repo: String = consumer_row.try_get("repo_url").map_err(ApiError::Db)?;
        let consumer_team: String = consumer_row.try_get("owner_team").map_err(ApiError::Db)?;
        let consumer_contact: String = consumer_row.try_get("contact").map_err(ApiError::Db)?;

        // Query usage_event: op-level changes match any usage of that operation;
        // field-level-only changes require (operation AND field_path) to avoid
        // flagging consumers that never accessed the specific changed field.
        // Collect up to 5 matching evidence items per consumer.
        let mut evidence_items: Vec<Value> = Vec::new();

        if !op_level_ops.is_empty() || !field_level_only.is_empty() {
            let mut sql = String::from(
                "SELECT operation, field_path, recorded_at FROM usage_event \
                 WHERE consumer_id = ? AND service_id = ? AND recorded_at >= ? AND (",
            );
            let mut first = true;
            for _ in &op_level_ops {
                if !first { sql.push_str(" OR "); }
                sql.push_str("operation = ?");
                first = false;
            }
            for _ in &field_level_only {
                if !first { sql.push_str(" OR "); }
                sql.push_str("(operation = ? AND field_path = ?)");
                first = false;
            }
            sql.push_str(") ORDER BY recorded_at DESC LIMIT 5");

            let mut q = sqlx::query(&sql)
                .bind(&consumer_id)
                .bind(&service_id)
                .bind(&cutoff_30);
            for op in &op_level_ops {
                q = q.bind(op);
            }
            for (op, fp) in &field_level_only {
                q = q.bind(op);
                q = q.bind(fp);
            }

            for row in q.fetch_all(&pool).await? {
                use sqlx::Row as _;
                let op: String = row.try_get("operation").unwrap_or_default();
                let fp: Option<String> = row.try_get("field_path").ok().flatten();
                let ts: String = row.try_get("recorded_at").unwrap_or_default();
                evidence_items.push(json!({
                    "kind":        "runtime_usage",
                    "operation":   op,
                    "field_path":  fp,
                    "recorded_at": ts,
                }));
            }
        }

        // Query call_site: op-level matches by operation; field-level matches by field_path
        // (static scanners often record field names without operation context).
        let changed_field_paths: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            changed_fields
                .iter()
                .map(|(_, fp)| fp.clone())
                .filter(|fp| !fp.is_empty() && seen.insert(fp.clone()))
                .collect()
        };

        if !op_level_ops.is_empty() || !changed_field_paths.is_empty() {
            let mut sql = String::from(
                "SELECT operation, field_path, file_path, line_number, last_seen_at \
                 FROM call_site WHERE consumer_id = ? AND service_id = ? AND (",
            );
            let mut first = true;
            for _ in &op_level_ops {
                if !first { sql.push_str(" OR "); }
                sql.push_str("operation = ?");
                first = false;
            }
            for _ in &changed_field_paths {
                if !first { sql.push_str(" OR "); }
                sql.push_str("field_path = ?");
                first = false;
            }
            sql.push_str(") ORDER BY last_seen_at DESC LIMIT 5");

            let mut q = sqlx::query(&sql).bind(&consumer_id).bind(&service_id);
            for op in &op_level_ops {
                q = q.bind(op);
            }
            for fp in &changed_field_paths {
                q = q.bind(fp);
            }

            for row in q.fetch_all(&pool).await? {
                use sqlx::Row as _;
                let op: String = row.try_get("operation").unwrap_or_default();
                let fp: Option<String> = row.try_get("field_path").ok().flatten();
                let fp_val = fp.filter(|s| !s.is_empty());
                let file: String = row.try_get("file_path").unwrap_or_default();
                let line: i64 = row.try_get("line_number").unwrap_or(0);
                let ts: String = row.try_get("last_seen_at").unwrap_or_default();
                evidence_items.push(json!({
                    "kind":         "call_site",
                    "operation":    op,
                    "field_path":   fp_val,
                    "file_path":    file,
                    "line_number":  line,
                    "last_seen_at": ts,
                }));
            }
        }

        // Skip consumers with no evidence of using the changed paths.
        if evidence_items.is_empty() {
            continue;
        }

        let has_runtime_usage = evidence_items.iter().any(|e| e["kind"] == "runtime_usage");
        let has_call_site = evidence_items.iter().any(|e| e["kind"] == "call_site");

        // Derive timestamps from evidence for confidence and last_seen calculations.
        let usage_last_seen: Option<String> = evidence_items.iter()
            .filter(|e| e["kind"] == "runtime_usage")
            .filter_map(|e| e["recorded_at"].as_str().map(|s| s.to_string()))
            .max();
        let call_site_last_seen: Option<String> = evidence_items.iter()
            .filter(|e| e["kind"] == "call_site")
            .filter_map(|e| e["last_seen_at"].as_str().map(|s| s.to_string()))
            .max();

        // Determine confidence.
        let confidence = if let Some(ref ts) = usage_last_seen {
            if ts.as_str() >= cutoff_7.as_str() {
                "high"
            } else {
                "medium"
            }
        } else {
            "low"
        };

        // Determine last_seen: prefer usage_event, fallback to call_site.
        let last_seen = usage_last_seen
            .or(call_site_last_seen)
            .unwrap_or_default();

        entries.push(json!({
            "consumer": {
                "id":         consumer_id,
                "name":       consumer_name,
                "repo_url":   consumer_repo,
                "owner_team": consumer_team,
                "contact":    consumer_contact,
            },
            "confidence":        confidence,
            "last_seen":         last_seen,
            "has_runtime_usage": has_runtime_usage,
            "has_call_site":     has_call_site,
            "evidence":          evidence_items,
        }));
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "diff_id":      diff_id,
            "service_id":   service_id,
            "lookback_days": lookback_days,
            "entries":      entries,
        })),
    ))
}

// POST /v1/usage/events
async fn ingest_usage_event(
    State(pool): State<sqlx::AnyPool>,
    Json(events): Json<Vec<UsageEventRequest>>,
) -> Result<impl IntoResponse, ApiError> {
    if events.len() > 500 {
        return Err(ApiError::TooManyRequests(
            "batch too large, max 500".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let count = events.len();

    for event in &events {
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
        .execute(&pool)
        .await?;
    }

    Ok((StatusCode::ACCEPTED, Json(json!({"accepted": count}))))
}

// POST /v1/call-sites — upsert static call site records from the radar-scanner.
async fn upsert_call_sites(
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

    for site in &sites {
        let id = call_site_id(
            &site.consumer_id,
            &site.service_id,
            &site.file_path,
            site.line_number,
            &site.field_path,
        );

        // UPDATE first; INSERT when the record did not exist yet.
        let updated = sqlx::query(
            "UPDATE call_site SET last_seen_at = ?, operation = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(&site.operation)
        .bind(&id)
        .execute(&pool)
        .await?;

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
            .execute(&pool)
            .await?;
        }
    }

    Ok((StatusCode::ACCEPTED, Json(json!({"accepted": count}))))
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

// GET /v1/diffs — all diffs across all services, paginated (?limit=50&offset=0)
async fn list_all_diffs(
    State(pool): State<sqlx::AnyPool>,
    Query(page): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let limit = page.limit.clamp(1, 200);
    let offset = page.offset.max(0);

    let rows = sqlx::query(
        r#"
        SELECT
            d.id            AS diff_id,
            sv_from.git_ref AS from_git_ref,
            sv_to.git_ref   AS to_git_ref,
            s.id            AS service_id,
            s.name          AS service_name,
            d.pr_url,
            d.created_at,
            (SELECT COUNT(*) FROM change c WHERE c.diff_id = d.id AND c.severity = 'breaking')           AS breaking_count,
            (SELECT COUNT(*) FROM change c WHERE c.diff_id = d.id AND c.severity = 'non_breaking_risky') AS risky_count,
            (SELECT COUNT(*) FROM change c WHERE c.diff_id = d.id AND c.severity = 'safe')               AS safe_count
        FROM diff d
        JOIN spec_version sv_from ON sv_from.id = d.from_version
        JOIN spec_version sv_to   ON sv_to.id   = d.to_version
        JOIN service s            ON s.id        = sv_to.service_id
        ORDER BY d.created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":            row.get::<String, _>("diff_id"),
                "service_id":    row.get::<String, _>("service_id"),
                "service_name":  row.get::<String, _>("service_name"),
                "from_git_ref":  row.get::<String, _>("from_git_ref"),
                "to_git_ref":    row.get::<String, _>("to_git_ref"),
                "pr_url":        row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
                "created_at":    row.get::<String, _>("created_at"),
                "breaking_count": row.try_get::<i64, _>("breaking_count").unwrap_or(0),
                "risky_count":    row.try_get::<i64, _>("risky_count").unwrap_or(0),
                "safe_count":     row.try_get::<i64, _>("safe_count").unwrap_or(0),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// POST /v1/services — explicitly register a Producer service
async fn create_service(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateServiceBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.is_empty() {
        return Err(ApiError::BadRequest("name is required".into()));
    }
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let id = body.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    sqlx::query(
        r#"
        INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name        = excluded.name,
            repo_url    = excluded.repo_url,
            owner_team  = excluded.owner_team,
            spec_format = excluded.spec_format
        "#,
    )
    .bind(&id)
    .bind(&body.name)
    .bind(&body.repo_url)
    .bind(&body.owner_team)
    .bind(&body.spec_format)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":          id,
            "name":        body.name,
            "repo_url":    body.repo_url,
            "owner_team":  body.owner_team,
            "spec_format": body.spec_format,
        })),
    ))
}

// GET /v1/services/:id — fetch a single Producer service by ID
async fn get_service(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let caller_org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let row = sqlx::query(
        "SELECT id, name, repo_url, owner_team, spec_format, org_id FROM service WHERE id = ?",
    )
    .bind(&service_id)
    .fetch_optional(&pool)
    .await?;

    match row {
        None => Err(ApiError::NotFound(format!("service {service_id} not found"))),
        Some(r) => {
            let row_org_id: String = r.try_get("org_id").unwrap_or_default();
            if !caller_org_id.is_empty() && row_org_id != caller_org_id {
                return Err(ApiError::NotFound(format!("service {service_id} not found")));
            }
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id":          r.get::<String, _>("id"),
                    "name":        r.get::<String, _>("name"),
                    "repo_url":    r.get::<String, _>("repo_url"),
                    "owner_team":  r.get::<String, _>("owner_team"),
                    "spec_format": r.get::<String, _>("spec_format"),
                })),
            ))
        }
    }
}

// GET /v1/services — list all registered Producer services
async fn list_services(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let rows = if !org_id.is_empty() {
        sqlx::query(
            "SELECT id, name, repo_url, owner_team, spec_format FROM service WHERE org_id = ? ORDER BY name",
        )
        .bind(&org_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            "SELECT id, name, repo_url, owner_team, spec_format FROM service ORDER BY name",
        )
        .fetch_all(&pool)
        .await?
    };

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":          row.get::<String, _>("id"),
                "name":        row.get::<String, _>("name"),
                "repo_url":    row.get::<String, _>("repo_url"),
                "owner_team":  row.get::<String, _>("owner_team"),
                "spec_format": row.get::<String, _>("spec_format"),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// GET /v1/consumers — list all registered Consumer services
async fn list_all_consumers(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let rows = if !org_id.is_empty() {
        sqlx::query(
            r#"
            SELECT
                c.id, c.name, c.repo_url, c.owner_team, c.contact,
                (SELECT COUNT(*) FROM subscription s WHERE s.consumer_id = c.id)         AS subscription_count,
                (SELECT MAX(recorded_at) FROM usage_event ue WHERE ue.consumer_id = c.id) AS last_seen
            FROM consumer c
            WHERE c.org_id = ?
            ORDER BY c.name
            "#,
        )
        .bind(&org_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT
                c.id, c.name, c.repo_url, c.owner_team, c.contact,
                (SELECT COUNT(*) FROM subscription s WHERE s.consumer_id = c.id)         AS subscription_count,
                (SELECT MAX(recorded_at) FROM usage_event ue WHERE ue.consumer_id = c.id) AS last_seen
            FROM consumer c
            ORDER BY c.name
            "#,
        )
        .fetch_all(&pool)
        .await?
    };

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":                 row.get::<String, _>("id"),
                "name":               row.get::<String, _>("name"),
                "repo_url":           row.get::<String, _>("repo_url"),
                "owner_team":         row.get::<String, _>("owner_team"),
                "contact":            row.get::<String, _>("contact"),
                "subscription_count": row.try_get::<i64, _>("subscription_count").unwrap_or(0),
                "last_seen":          row.try_get::<Option<String>, _>("last_seen").unwrap_or(None),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// GET /v1/summary — KPI stats for the dashboard home page
async fn get_summary(
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

// ---------------------------------------------------------------------------
// POST /v1/generate-tests — generate a Postman Collection from a Jira ticket + spec
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GenerateTestsBody {
    /// OpenAPI YAML/JSON spec text. Either this or diff_id is required.
    spec_yaml: Option<String>,
    /// Diff ID — if provided and spec_yaml is absent, the spec is loaded from the stored
    /// spec_version for this diff's to_version.
    diff_id: Option<String>,
    /// Jira ticket key (e.g. "PROJ-123"). Server reads JIRA_BASE_URL/JIRA_EMAIL/JIRA_TOKEN.
    jira_key: Option<String>,
    /// Paste the Jira ticket text directly (used when jira_key is absent or Jira is unreachable).
    jira_text: Option<String>,
    /// Optional service UUID to associate this suite with.
    service_id: Option<String>,
    /// Base URL inserted into generated requests (default: http://localhost:8080).
    #[serde(default = "default_base_url")]
    base_url: String,
}

fn default_base_url() -> String {
    "http://localhost:8080".to_string()
}

async fn generate_tests(
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<GenerateTestsBody>,
) -> Result<impl IntoResponse, ApiError> {
    // Resolve Jira content.
    let (jira_summary, jira_description) = match body.jira_key {
        Some(ref key) => {
            let result = fetch_jira_ticket(key).await;
            match result {
                Ok((s, d)) => (s, d),
                Err(e) => {
                    // Fall back to jira_text if Jira API fails.
                    if let Some(text) = body.jira_text.clone() {
                        let first = text.lines().next().unwrap_or("").to_string();
                        (first, text)
                    } else {
                        return Err(ApiError::BadRequest(format!(
                            "Jira fetch failed and no jira_text provided: {e}"
                        )));
                    }
                }
            }
        }
        None => match body.jira_text.clone() {
            Some(text) => {
                let first = text.lines().next().unwrap_or("").to_string();
                (first, text)
            }
            None => {
                return Err(ApiError::BadRequest(
                    "Provide either jira_key or jira_text".to_string(),
                ))
            }
        },
    };

    // Resolve spec_yaml: use explicit value, or fall back to stored spec from the diff.
    let spec_yaml = match body.spec_yaml {
        Some(s) => s,
        None => {
            match body.diff_id {
                Some(ref did) => {
                    use sqlx::Row;
                    let row = sqlx::query(
                        r#"SELECT sv.spec_yaml FROM diff d
                           JOIN spec_version sv ON sv.id = d.to_version
                           WHERE d.id = ?"#,
                    )
                    .bind(did)
                    .fetch_optional(&pool)
                    .await?;
                    match row.and_then(|r| r.try_get::<Option<String>, _>("spec_yaml").ok().flatten()) {
                        Some(yaml) => yaml,
                        None => return Err(ApiError::BadRequest(
                            "No stored spec found for this diff. Supply spec_yaml directly.".to_string()
                        )),
                    }
                }
                None => return Err(ApiError::BadRequest(
                    "Provide either spec_yaml or diff_id".to_string()
                )),
            }
        }
    };

    // Call the configured AI provider; assemble both Postman JSON and api-testing YAML from the result.
    let suite_raw =
        call_ai_for_tests(&jira_summary, &jira_description, &spec_yaml)
            .await
            .map_err(|e| ApiError::BadRequest(format!("test generation failed: {e}")))?;

    let (collection_json, apitesting_yaml) = build_both_formats(suite_raw, &body.base_url);

    // Parse counts from the Postman collection.
    let items = collection_json["item"].as_array();
    let test_count = items.map(|a| a.len()).unwrap_or(0) as i64;
    let happy_count = items
        .map(|a| a.iter().filter(|i| i["name"].as_str().unwrap_or("").starts_with("[HAPPY")).count())
        .unwrap_or(0) as i64;
    let negative_count = test_count - happy_count;
    let collection_name = collection_json["info"]["name"].as_str().unwrap_or("Generated Tests").to_string();

    // Persist both formats.
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let collection_str = serde_json::to_string(&collection_json).unwrap_or_default();

    sqlx::query(
        r#"INSERT INTO generated_test_suite
           (id, service_id, jira_key, jira_summary, collection_name, collection_json,
            test_count, happy_count, negative_count, created_at, apitesting_yaml)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.service_id)
    .bind(&body.jira_key)
    .bind(&jira_summary)
    .bind(&collection_name)
    .bind(&collection_str)
    .bind(test_count)
    .bind(happy_count)
    .bind(negative_count)
    .bind(&now)
    .bind(&apitesting_yaml)
    .execute(&pool)
    .await?;

    metrics::counter!("radar_test_suites_created_total").increment(1);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":               id,
            "collection_name":  collection_name,
            "test_count":       test_count,
            "happy_count":      happy_count,
            "negative_count":   negative_count,
            "collection_json":  collection_json,
            "apitesting_yaml":  apitesting_yaml,
            "created_at":       now,
        })),
    ))
}

// GET /v1/generate-tests — list previously generated test suites, paginated
async fn list_test_suites(
    State(pool): State<sqlx::AnyPool>,
    Query(page): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let limit = page.limit.clamp(1, 200);
    let offset = page.offset.max(0);

    let rows = sqlx::query(
        r#"SELECT id, service_id, jira_key, jira_summary, collection_name,
                  test_count, happy_count, negative_count, created_at
           FROM generated_test_suite
           ORDER BY created_at DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":              r.get::<String, _>("id"),
                "service_id":      r.try_get::<Option<String>, _>("service_id").unwrap_or(None),
                "jira_key":        r.try_get::<Option<String>, _>("jira_key").unwrap_or(None),
                "jira_summary":    r.try_get::<Option<String>, _>("jira_summary").unwrap_or(None),
                "collection_name": r.get::<String, _>("collection_name"),
                "test_count":      r.try_get::<i64, _>("test_count").unwrap_or(0),
                "happy_count":     r.try_get::<i64, _>("happy_count").unwrap_or(0),
                "negative_count":  r.try_get::<i64, _>("negative_count").unwrap_or(0),
                "created_at":      r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// GET /v1/generate-tests/:id — fetch a single test suite with full collection JSON
async fn get_test_suite(
    Path(suite_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT id, service_id, jira_key, jira_summary, collection_name,
                  collection_json, apitesting_yaml, test_count, happy_count, negative_count, created_at
           FROM generated_test_suite
           WHERE id = ?"#,
    )
    .bind(&suite_id)
    .fetch_optional(&pool)
    .await?;

    match row {
        None => Err(ApiError::NotFound(format!("test suite {suite_id} not found"))),
        Some(r) => {
            let collection_json: Value = r
                .try_get::<String, _>("collection_json")
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            let apitesting_yaml = r.try_get::<Option<String>, _>("apitesting_yaml").unwrap_or(None);
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id":               r.get::<String, _>("id"),
                    "service_id":       r.try_get::<Option<String>, _>("service_id").unwrap_or(None),
                    "jira_key":         r.try_get::<Option<String>, _>("jira_key").unwrap_or(None),
                    "jira_summary":     r.try_get::<Option<String>, _>("jira_summary").unwrap_or(None),
                    "collection_name":  r.get::<String, _>("collection_name"),
                    "collection_json":  collection_json,
                    "apitesting_yaml":  apitesting_yaml,
                    "test_count":       r.try_get::<i64, _>("test_count").unwrap_or(0),
                    "happy_count":      r.try_get::<i64, _>("happy_count").unwrap_or(0),
                    "negative_count":   r.try_get::<i64, _>("negative_count").unwrap_or(0),
                    "created_at":       r.get::<String, _>("created_at"),
                })),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for generate-tests
// ---------------------------------------------------------------------------

async fn fetch_jira_ticket(key: &str) -> anyhow::Result<(String, String)> {
    let base = std::env::var("JIRA_BASE_URL")
        .map_err(|_| anyhow::anyhow!("JIRA_BASE_URL not set"))?;
    let email = std::env::var("JIRA_EMAIL")
        .map_err(|_| anyhow::anyhow!("JIRA_EMAIL not set"))?;
    let token = std::env::var("JIRA_TOKEN")
        .map_err(|_| anyhow::anyhow!("JIRA_TOKEN not set"))?;

    let url = format!("{}/rest/api/2/issue/{}", base.trim_end_matches('/'), key);
    let resp = reqwest::Client::new()
        .get(&url)
        .basic_auth(&email, Some(&token))
        .send()
        .await?
        .error_for_status()?;

    let body: Value = resp.json().await?;
    let fields = &body["fields"];
    let summary = fields["summary"].as_str().unwrap_or("").to_string();
    let description = fields["description"].as_str().unwrap_or("").to_string();
    Ok((summary, description))
}

async fn call_ai_for_tests(
    jira_summary: &str,
    jira_description: &str,
    spec_yaml: &str,
) -> anyhow::Result<Value> {
    let spec_excerpt = if spec_yaml.len() > 40_000 { &spec_yaml[..40_000] } else { spec_yaml };

    let prompt = format!(
        r#"You are a QA engineer generating Postman API tests from a Jira ticket and an OpenAPI spec.

## Jira Ticket
Title: {jira_summary}
Description:
{jira_description}

## OpenAPI Specification
```yaml
{spec_excerpt}
```

## Task
Generate API test cases:
1. Happy-path tests — valid inputs satisfying the ticket's acceptance criteria
2. Negative tests — missing required fields (→ 400/422), wrong types (→ 400), unauthorized (→ 401), not-found (→ 404)

Rules:
- Use {{{{baseUrl}}}} as the host placeholder and {{{{authToken}}}} for bearer auth
- Each assertion is a complete valid JavaScript pm.test() statement on a single line
- Aim for 4–6 happy-path and 4–8 negative tests
- Return ONLY valid JSON — no markdown fences, no surrounding text

Required JSON format:
{{
  "collection_name": "TICKET-KEY — Short Title",
  "test_cases": [
    {{
      "name": "Happy Path — create resource",
      "category": "happy_path",
      "method": "POST",
      "path": "/v1/resource",
      "path_params": {{}},
      "query_params": {{}},
      "body": {{"field": "value"}},
      "expected_status": 201,
      "assertions": [
        "pm.test('Response has id', () => {{ pm.expect(pm.response.json()).to.have.property('id'); }});"
      ]
    }}
  ]
}}"#
    );

    let raw_text = detect_provider()
        .ok_or_else(|| anyhow::anyhow!("No AI provider configured (set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GITHUB_COPILOT_TOKEN)"))?
        .complete(&prompt, 4096)
        .await
        .ok_or_else(|| anyhow::anyhow!("AI provider call failed"))?;

    let start = raw_text.find('{').ok_or_else(|| anyhow::anyhow!("no JSON in response"))?;
    let end = raw_text.rfind('}').ok_or_else(|| anyhow::anyhow!("no JSON in response"))?;
    let suite: Value = serde_json::from_str(&raw_text[start..=end])?;
    Ok(suite)
}

// ---------------------------------------------------------------------------
// Inline AI provider — mirrors radar-cli/src/ai_provider.rs
// (radar-api is a separate crate; duplication is intentional)
// ---------------------------------------------------------------------------

enum AiProvider {
    Anthropic { api_key: String },
    OpenAI { api_key: String, base_url: String },
    GitHubCopilot { token: String },
}

fn detect_provider() -> Option<AiProvider> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        if !k.is_empty() {
            return Some(AiProvider::Anthropic { api_key: k });
        }
    }
    if let Ok(k) = std::env::var("OPENAI_API_KEY") {
        if !k.is_empty() {
            let base = std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into());
            return Some(AiProvider::OpenAI { api_key: k, base_url: base });
        }
    }
    if let Ok(t) = std::env::var("GITHUB_COPILOT_TOKEN") {
        if !t.is_empty() {
            return Some(AiProvider::GitHubCopilot { token: t });
        }
    }
    None
}

impl AiProvider {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Option<String> {
        match self {
            Self::Anthropic { api_key } => {
                ai_call_anthropic(api_key, prompt, max_tokens).await
            }
            Self::OpenAI { api_key, base_url } => {
                ai_call_openai_compat(api_key, base_url, prompt, max_tokens).await
            }
            Self::GitHubCopilot { token } => {
                ai_call_openai_compat(token, "https://api.githubcopilot.com/v1", prompt, max_tokens).await
            }
        }
    }
}

async fn ai_call_anthropic(api_key: &str, prompt: &str, max_tokens: u32) -> Option<String> {
    let body = json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}]
    });
    let resp = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!("Anthropic API error: {}", resp.status());
        return None;
    }
    let data: Value = resp.json().await.ok()?;
    data["content"].as_array()?
        .iter()
        .find(|b| b["type"] == "text")
        .and_then(|b| b["text"].as_str())
        .map(str::to_owned)
}

async fn ai_call_openai_compat(api_key: &str, base_url: &str, prompt: &str, max_tokens: u32) -> Option<String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = json!({
        "model": "gpt-4o",
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}]
    });
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!("OpenAI-compat API error {}: {}", url, resp.status());
        return None;
    }
    let data: Value = resp.json().await.ok()?;
    data["choices"].as_array()?.first()
        .and_then(|c| c["message"]["content"].as_str())
        .map(str::to_owned)
}

fn build_both_formats(suite: Value, base_url: &str) -> (Value, String) {
    let apitesting_yaml = assemble_apitesting_yaml(&suite, base_url);
    let postman = assemble_postman_collection(suite, base_url);
    (postman, apitesting_yaml)
}

fn assemble_postman_collection(suite: Value, base_url: &str) -> Value {
    let collection_name = suite["collection_name"].as_str().unwrap_or("Generated Tests").to_string();
    let empty = vec![];
    let test_cases = suite["test_cases"].as_array().unwrap_or(&empty);

    let items: Vec<Value> = test_cases.iter().map(|tc| {
        let category = tc["category"].as_str().unwrap_or("test");
        let name = tc["name"].as_str().unwrap_or("Test");
        let method = tc["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = tc["path"].as_str().unwrap_or("/");
        let expected_status = tc["expected_status"].as_u64().unwrap_or(200);

        let path_segs: Vec<Value> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty())
            .map(|s| Value::String(s.to_string())).collect();

        let mut assertions = vec![
            format!("pm.test('Status is {expected_status}', () => pm.response.to.have.status({expected_status}));"),
        ];
        if let Some(arr) = tc["assertions"].as_array() {
            for a in arr {
                if let Some(s) = a.as_str() {
                    if !s.contains("have.status") {
                        assertions.push(s.to_string());
                    }
                }
            }
        }

        let has_body = !tc["body"].is_null() && tc["body"].is_object();
        let mut headers = vec![
            json!({"key": "Authorization", "value": "Bearer {{authToken}}", "type": "text"}),
        ];
        if has_body {
            headers.push(json!({"key": "Content-Type", "value": "application/json", "type": "text"}));
        }

        let body_json = if has_body {
            json!({
                "mode": "raw",
                "raw": serde_json::to_string_pretty(&tc["body"]).unwrap_or_default(),
                "options": {"raw": {"language": "json"}}
            })
        } else {
            Value::Null
        };

        let label = format!("[{}] {}", category.replace('_', " ").to_uppercase(), name);

        let mut item = json!({
            "name": label,
            "event": [{
                "listen": "test",
                "script": {
                    "type": "text/javascript",
                    "exec": assertions
                }
            }],
            "request": {
                "method": method,
                "header": headers,
                "url": {
                    "raw": format!("{{{{baseUrl}}}}{path}"),
                    "host": ["{{baseUrl}}"],
                    "path": path_segs,
                    "query": []
                }
            }
        });

        if !body_json.is_null() {
            item["request"]["body"] = body_json;
        }

        item
    }).collect();

    json!({
        "info": {
            "name": collection_name,
            "_postman_id": Uuid::new_v4().to_string(),
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": items,
        "variable": [
            {"key": "baseUrl", "value": base_url, "type": "string"},
            {"key": "authToken", "value": "", "type": "string"}
        ]
    })
}

/// Build an api-testing YAML suite from the raw Claude JSON value.
/// Internalises format patterns from https://github.com/LinuxSuRen/api-testing:
/// - `#!api-testing` magic header for auto-detection
/// - `param:` block with authToken for `{{.param.authToken}}` templating
/// - `expect.verify:` using the expr library (`data.field != null`)
/// - `expect.bodyFieldsExpect:` for simple field=value pins on happy-path tests
fn assemble_apitesting_yaml(suite: &Value, base_url: &str) -> String {
    #[derive(serde::Serialize)]
    struct Suite<'a> {
        name: &'a str,
        api: &'a str,
        param: std::collections::BTreeMap<&'static str, &'static str>,
        spec: Spec,
        items: Vec<TestCase>,
    }
    #[derive(serde::Serialize)]
    struct Spec { kind: &'static str }
    #[derive(serde::Serialize)]
    struct TestCase {
        name: String,
        request: Request,
        expect: Expect,
    }
    #[derive(serde::Serialize)]
    struct Request {
        api: String,
        method: String,
        header: std::collections::BTreeMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    }
    #[derive(serde::Serialize)]
    struct Expect {
        #[serde(rename = "statusCode")]
        status_code: u64,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        verify: Vec<String>,
        #[serde(rename = "bodyFieldsExpect", skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        body_fields_expect: std::collections::BTreeMap<String, Value>,
    }

    let collection_name = suite["collection_name"].as_str().unwrap_or("Generated Tests");
    let empty = vec![];
    let test_cases = suite["test_cases"].as_array().unwrap_or(&empty);

    let mut param = std::collections::BTreeMap::new();
    param.insert("authToken", "");

    let items: Vec<TestCase> = test_cases.iter().map(|tc| {
        let category = tc["category"].as_str().unwrap_or("test");
        let name = tc["name"].as_str().unwrap_or("Test");
        let method = tc["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = tc["path"].as_str().unwrap_or("/").to_string();
        let status = tc["expected_status"].as_u64().unwrap_or(200);
        let has_body = tc["body"].is_object() && !tc["body"].as_object().map(|m| m.is_empty()).unwrap_or(true);

        let mut header = std::collections::BTreeMap::new();
        header.insert("Authorization".into(), "Bearer {{.param.authToken}}".into());
        if has_body {
            header.insert("Content-Type".into(), "application/json".into());
        }

        let body = if has_body {
            Some(serde_json::to_string_pretty(&tc["body"]).unwrap_or_default())
        } else {
            None
        };

        // Convert Postman assertions to api-testing expr verify expressions.
        let mut verify: Vec<String> = tc["assertions"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(|a| postman_assertion_to_verify(a.as_str().unwrap_or("")))
            .collect();

        if verify.is_empty() {
            verify.push(if category == "happy_path" {
                "data != null".into()
            } else {
                "data.error != null".into()
            });
        }

        // bodyFieldsExpect: pin top-level scalar fields from the request body for
        // happy-path tests as a lightweight contract check.
        let body_fields_expect = if category == "happy_path" {
            tc["body"].as_object()
                .map(|m| m.iter()
                    .filter(|(_, v)| v.is_string() || v.is_number() || v.is_boolean())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect())
                .unwrap_or_default()
        } else {
            std::collections::BTreeMap::new()
        };

        let label = format!("[{}] {name}", category.replace('_', " ").to_uppercase());
        TestCase {
            name: label,
            request: Request { api: path, method, header, body },
            expect: Expect { status_code: status, verify, body_fields_expect },
        }
    }).collect();

    let s = Suite { name: collection_name, api: base_url, param, spec: Spec { kind: "openapi" }, items };
    match serde_yaml::to_string(&s) {
        Ok(yaml) => format!("#!api-testing\n{yaml}"),
        Err(_) => String::from("#!api-testing\n# (yaml serialisation failed)\n"),
    }
}

/// Convert a Postman pm.test() assertion line to an api-testing expr verify expression.
fn postman_assertion_to_verify(assertion: &str) -> Option<String> {
    if assertion.contains("have.status") { return None; }
    if assertion.contains("headers.get") || assertion.contains("response.headers") { return None; }

    for q in ["'", "\""] {
        let pat = format!(".have.property({q}");
        if let Some(pos) = assertion.find(&pat) {
            let rest = &assertion[pos + pat.len()..];
            if let Some(end) = rest.find(q) {
                let field = &rest[..end];
                if !field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return Some(format!("data.{field} != null"));
                }
            }
        }
    }
    if assertion.contains(".to.have.length.above(0)") || assertion.contains("lengthOf.above(0)") {
        return Some("len(data) > 0".into());
    }
    None
}

// ---------------------------------------------------------------------------
// Retention job (public, callable on a schedule)
// ---------------------------------------------------------------------------

/// Delete usage_event rows older than `lookback_days` days.
// ---------------------------------------------------------------------------
// Release note handlers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct CreateReleaseNoteBody {
    content: String,
}

// POST /v1/diffs/:id/release-notes
async fn create_release_note(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<CreateReleaseNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.content.is_empty() {
        return Err(ApiError::BadRequest("content is required".into()));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO release_note (id, diff_id, content, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&diff_id)
    .bind(&body.content)
    .bind(&now)
    .execute(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "diff_id": diff_id, "created_at": now }))))
}

// GET /v1/release-notes
async fn list_release_notes(
    State(pool): State<sqlx::AnyPool>,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    let rows = sqlx::query(
        r#"SELECT rn.id, rn.diff_id, rn.created_at,
                  d.from_version, d.to_version,
                  sv_from.git_ref AS from_git_ref,
                  sv_to.git_ref   AS to_git_ref
           FROM release_note rn
           JOIN diff        d      ON d.id      = rn.diff_id
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           ORDER BY rn.created_at DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(params.limit)
    .bind(params.offset)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows.iter().map(|r| {
        use sqlx::Row;
        json!({
            "id":           r.get::<String, _>("id"),
            "diff_id":      r.get::<String, _>("diff_id"),
            "from_git_ref": r.get::<String, _>("from_git_ref"),
            "to_git_ref":   r.get::<String, _>("to_git_ref"),
            "created_at":   r.get::<String, _>("created_at"),
        })
    }).collect();

    Ok(Json(json!(items)))
}

// GET /v1/release-notes/:id
async fn get_release_note(
    Path(note_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    let row = sqlx::query(
        r#"SELECT rn.id, rn.diff_id, rn.content, rn.created_at,
                  sv_from.git_ref AS from_git_ref,
                  sv_to.git_ref   AS to_git_ref
           FROM release_note rn
           JOIN diff        d      ON d.id        = rn.diff_id
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           WHERE rn.id = ?"#,
    )
    .bind(&note_id)
    .fetch_optional(&pool)
    .await?;

    match row {
        None => Err(ApiError::NotFound(format!("release note {note_id} not found"))),
        Some(r) => {
            use sqlx::Row;
            Ok(Json(json!({
                "id":           r.get::<String, _>("id"),
                "diff_id":      r.get::<String, _>("diff_id"),
                "from_git_ref": r.get::<String, _>("from_git_ref"),
                "to_git_ref":   r.get::<String, _>("to_git_ref"),
                "content":      r.get::<String, _>("content"),
                "created_at":   r.get::<String, _>("created_at"),
            })))
        }
    }
}

// ---------------------------------------------------------------------------
// Settings handlers
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct AppSettings {
    policy_block_on: String,
    policy_lookback_days: i64,
    policy_allow_override_with: Option<String>,
    retention_days: i64,
}

// ---------------------------------------------------------------------------
// Sandbox Environments — shared Playground environments
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct SandboxEnvBody {
    name: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    bearer_token: String,
    #[serde(default)]
    description: String,
}

fn mask_token(t: &str) -> String {
    if t.len() <= 4 {
        "***".into()
    } else {
        format!("***{}", &t[t.len() - 4..])
    }
}

// GET /v1/sandbox-envs
async fn list_sandbox_envs(
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, base_url, bearer_token, description, created_at, updated_at \
         FROM sandbox_env ORDER BY name ASC",
    )
    .fetch_all(&pool)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let raw_token: String = r.try_get("bearer_token").unwrap_or_default();
            json!({
                "id":               r.try_get::<String, _>("id").unwrap_or_default(),
                "name":             r.try_get::<String, _>("name").unwrap_or_default(),
                "base_url":         r.try_get::<String, _>("base_url").unwrap_or_default(),
                "bearer_token":     mask_token(&raw_token),
                "bearer_token_set": !raw_token.is_empty(),
                "description":      r.try_get::<String, _>("description").unwrap_or_default(),
                "created_at":       r.try_get::<String, _>("created_at").unwrap_or_default(),
                "updated_at":       r.try_get::<String, _>("updated_at").unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(items))
}

// POST /v1/sandbox-envs
async fn create_sandbox_env(
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<SandboxEnvBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name is required".into()));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO sandbox_env (id, name, base_url, bearer_token, description, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.base_url.trim())
    .bind(&body.bearer_token)  // not trimmed — tokens may have significant whitespace
    .bind(body.description.trim())
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": body.name.trim(),
            "base_url": body.base_url.trim(),
            "bearer_token": mask_token(&body.bearer_token),
            "bearer_token_set": !body.bearer_token.is_empty(),
            "description": body.description.trim(),
            "created_at": now,
            "updated_at": now,
        })),
    ))
}

// PUT /v1/sandbox-envs/:id
async fn update_sandbox_env(
    State(pool): State<sqlx::AnyPool>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<SandboxEnvBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name is required".into()));
    }

    let now = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        "UPDATE sandbox_env \
         SET name = ?, base_url = ?, bearer_token = ?, description = ?, updated_at = ? \
         WHERE id = ?",
    )
    .bind(body.name.trim())
    .bind(body.base_url.trim())
    .bind(&body.bearer_token)
    .bind(body.description.trim())
    .bind(&now)
    .bind(&id)
    .execute(&pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("sandbox environment not found".into()));
    }

    Ok(Json(json!({
        "id": id,
        "name": body.name.trim(),
        "base_url": body.base_url.trim(),
        "bearer_token": mask_token(&body.bearer_token),
        "bearer_token_set": !body.bearer_token.is_empty(),
        "description": body.description.trim(),
        "updated_at": now,
    })))
}

// DELETE /v1/sandbox-envs/:id
async fn delete_sandbox_env(
    State(pool): State<sqlx::AnyPool>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let result = sqlx::query("DELETE FROM sandbox_env WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("sandbox environment not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Spec Versions — Playground support
// ---------------------------------------------------------------------------

// GET /v1/spec-versions
async fn list_spec_versions(
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"SELECT sv.id, sv.service_id, s.name AS service_name, sv.git_ref,
                  sv.spec_format, sv.captured_at
           FROM spec_version sv
           JOIN service s ON s.id = sv.service_id
           WHERE sv.spec_yaml IS NOT NULL
           ORDER BY sv.captured_at DESC
           LIMIT 100"#,
    )
    .fetch_all(&pool)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":           r.try_get::<String, _>("id").unwrap_or_default(),
                "service_id":   r.try_get::<String, _>("service_id").unwrap_or_default(),
                "service_name": r.try_get::<String, _>("service_name").unwrap_or_default(),
                "git_ref":      r.try_get::<String, _>("git_ref").unwrap_or_default(),
                "spec_format":  r.try_get::<String, _>("spec_format").unwrap_or_default(),
                "captured_at":  r.try_get::<String, _>("captured_at").unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(items))
}

// GET /v1/spec-versions/:id/raw
async fn get_spec_version_raw(
    State(pool): State<sqlx::AnyPool>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let row = sqlx::query("SELECT spec_yaml, spec_format FROM spec_version WHERE id = ?")
        .bind(&id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("spec version not found".into()))?;

    let spec_yaml: Option<String> = row.try_get("spec_yaml").ok().flatten();
    let spec_format: String = row.try_get("spec_format").unwrap_or_else(|_| "openapi".into());

    let content = spec_yaml.ok_or_else(|| ApiError::NotFound("no spec stored for this version".into()))?;

    let content_type = if spec_format.contains("json") || content.trim_start().starts_with('{') {
        "application/json"
    } else {
        "application/yaml"
    };

    Ok((
        [(axum::http::header::CONTENT_TYPE, content_type)],
        content,
    ))
}

// GET /v1/settings
async fn get_settings(State(pool): State<sqlx::AnyPool>) -> Result<impl IntoResponse, ApiError> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(&pool)
        .await?;

    let mut map: HashMap<String, String> = rows
        .iter()
        .map(|r| {
            use sqlx::Row;
            (r.get::<String, _>("key"), r.get::<String, _>("value"))
        })
        .collect();

    Ok(Json(AppSettings {
        policy_block_on: map
            .remove("policy.block_on")
            .unwrap_or_else(|| "active_consumers".to_string()),
        policy_lookback_days: map
            .remove("policy.lookback_days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
        policy_allow_override_with: map
            .remove("policy.allow_override_with")
            .filter(|s| !s.is_empty()),
        retention_days: map
            .remove("retention.days")
            .and_then(|v| v.parse().ok())
            .unwrap_or(90),
    }))
}

// PUT /v1/settings
async fn update_settings(
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<AppSettings>,
) -> Result<impl IntoResponse, ApiError> {
    if !["never", "any_break", "active_consumers"].contains(&body.policy_block_on.as_str()) {
        return Err(ApiError::BadRequest(
            "policy_block_on must be one of: never, any_break, active_consumers".into(),
        ));
    }
    if !(1..=365).contains(&body.policy_lookback_days) {
        return Err(ApiError::BadRequest(
            "policy_lookback_days must be between 1 and 365".into(),
        ));
    }
    if !(1..=3650).contains(&body.retention_days) {
        return Err(ApiError::BadRequest(
            "retention_days must be between 1 and 3650".into(),
        ));
    }

    let pairs = [
        ("policy.block_on", body.policy_block_on.clone()),
        (
            "policy.lookback_days",
            body.policy_lookback_days.to_string(),
        ),
        (
            "policy.allow_override_with",
            body.policy_allow_override_with.clone().unwrap_or_default(),
        ),
        ("retention.days", body.retention_days.to_string()),
    ];

    for (key, value) in &pairs {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await?;
    }

    Ok(Json(body))
}

// GET /v1/settings/integrations — checks env vars server-side, returns booleans only
async fn get_integrations() -> Json<Value> {
    let configured = |key: &str| std::env::var(key).map(|v| !v.is_empty()).unwrap_or(false);
    let openai_key = configured("OPENAI_API_KEY");
    Json(json!({
        "anthropic":         configured("ANTHROPIC_API_KEY"),
        "openai":            openai_key,
        "openai_enterprise": openai_key && configured("OPENAI_BASE_URL"),
        "github_copilot":    configured("GITHUB_COPILOT_TOKEN"),
        "jira":              configured("JIRA_BASE_URL") && configured("JIRA_EMAIL") && configured("JIRA_TOKEN"),
        "github":            configured("GITHUB_TOKEN"),
        "postman":           configured("POSTMAN_API_KEY"),
    }))
}

// ---------------------------------------------------------------------------

pub async fn purge_old_usage_events(pool: &sqlx::AnyPool, lookback_days: u32) -> anyhow::Result<u64> {
    let cutoff = (Utc::now() - Duration::days(lookback_days as i64)).to_rfc3339();
    let result = sqlx::query("DELETE FROM usage_event WHERE recorded_at < ?")
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
    use super::*;
    use axum::{
        body::Body,
        http::Request as HttpRequest,
    };
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    async fn test_pool() -> sqlx::AnyPool {
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to create test pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("failed to run migrations");
        // Disable FK enforcement so unit tests can insert usage_event rows freely.
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .unwrap();
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
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], diff_id);
        assert_eq!(arr[0]["service_name"], "list-api");
        assert_eq!(arr[0]["breaking_count"], 1);
        assert_eq!(arr[0]["safe_count"], 1);
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
        assert_eq!(json["breaking_changes_30d"], 2);
        assert_eq!(json["services_count"], 1);
        assert_eq!(json["consumers_at_risk"], 0);
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
}
