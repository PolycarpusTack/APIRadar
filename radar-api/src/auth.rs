use crate::errors::ApiError;
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::{
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Duration, Utc};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

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
pub(crate) fn validate_jwt(token: &str, secret: &str) -> Option<JwtClaims> {
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
        let org_claim = std::env::var("RADAR_OIDC_ORG_CLAIM").unwrap_or_else(|_| "hd".to_string());
        Some(OidcConfig {
            provider_url,
            client_id,
            client_secret,
            redirect_uri,
            org_claim,
        })
    }
}

#[derive(serde::Deserialize)]
struct OidcDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    userinfo_endpoint: Option<String>,
    /// Used to verify id_token signature when no userinfo_endpoint is available.
    /// Per OIDC Core §3.1.3.7, id_token signature MUST be verified before trusting any claim.
    #[serde(default)]
    jwks_uri: Option<String>,
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
pub(crate) fn sign_jwt(claims: &JwtClaims, secret: &str) -> Option<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .ok()
}

/// Sign an OidcState into an HS256 JWT string.
fn sign_state(state: &OidcState, secret: &str) -> Option<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    encode(
        &Header::new(Algorithm::HS256),
        state,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .ok()
}

/// Validate an OidcState JWT and return the nonce if valid.
fn validate_state(token: &str, secret: &str) -> Option<String> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut v = Validation::new(Algorithm::HS256);
    v.validate_exp = true;
    decode::<OidcState>(token, &key, &v)
        .ok()
        .map(|d| d.claims.nonce)
}

async fn fetch_discovery(provider_url: &str) -> anyhow::Result<OidcDiscovery> {
    let url = format!("{provider_url}/.well-known/openid-configuration");
    let disc: OidcDiscovery = reqwest::get(&url).await?.json().await?;
    Ok(disc)
}

pub(crate) fn parse_cookie(header: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    header.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(&prefix)
            .map(|v| v.trim().to_string())
    })
}

fn urlencoding_encode(s: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    // Encode all characters that are not unreserved (RFC 3986 §2.3).
    // Uses the percent-encoding crate for correct UTF-8 byte-level encoding.
    const UNRESERVED: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    utf8_percent_encode(s, UNRESERVED).to_string()
}

/// GET /auth/login — redirect to OIDC provider authorization endpoint.
pub(crate) async fn oidc_login() -> Response {
    let Some(cfg) = OidcConfig::from_env() else {
        return (StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "OIDC not configured — set RADAR_OIDC_PROVIDER_URL, RADAR_OIDC_CLIENT_ID, RADAR_OIDC_CLIENT_SECRET"}))).into_response();
    };
    let jwt_secret = match std::env::var("RADAR_JWT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
    {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "RADAR_JWT_SECRET must be set to use OIDC login"})),
            )
                .into_response()
        }
    };
    let disc = match fetch_discovery(&cfg.provider_url).await {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("OIDC discovery failed: {e}")})),
            )
                .into_response()
        }
    };
    let nonce = Uuid::new_v4().to_string();
    let state_claims = OidcState {
        nonce: nonce.clone(),
        exp: (Utc::now() + Duration::minutes(10)).timestamp() as usize,
    };
    let state_token = match sign_state(&state_claims, &jwt_secret) {
        Some(t) => t,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "state signing failed"})),
            )
                .into_response()
        }
    };
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid+email+profile&state={}",
        disc.authorization_endpoint,
        urlencoding_encode(&cfg.client_id),
        urlencoding_encode(&cfg.redirect_uri),
        urlencoding_encode(&state_token),
    );
    let state_cookie =
        format!("oidc_state={state_token}; HttpOnly; SameSite=Lax; Max-Age=600; Path=/");
    (
        StatusCode::FOUND,
        [(LOCATION, auth_url), (SET_COOKIE, state_cookie)],
    )
        .into_response()
}

/// GET /auth/callback?code=...&state=... — exchange code, issue session cookie.
pub(crate) async fn oidc_callback(
    Query(params): Query<HashMap<String, String>>,
    req: Request,
) -> Response {
    let Some(cfg) = OidcConfig::from_env() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "OIDC not configured"})),
        )
            .into_response();
    };
    let jwt_secret = match std::env::var("RADAR_JWT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
    {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "RADAR_JWT_SECRET must be set to use OIDC"})),
            )
                .into_response()
        }
    };

    // Verify CSRF state
    let state_param = params.get("state").cloned().unwrap_or_default();
    let cookie_header = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let state_cookie_val = parse_cookie(&cookie_header, "oidc_state");
    if state_cookie_val.as_deref() != Some(state_param.as_str())
        || validate_state(&state_param, &jwt_secret).is_none()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid or expired state"})),
        )
            .into_response();
    }

    let code = match params.get("code") {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "missing code"})),
            )
                .into_response()
        }
    };

    let disc = match fetch_discovery(&cfg.provider_url).await {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("OIDC discovery failed: {e}")})),
            )
                .into_response()
        }
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
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("token parse failed: {e}")})),
                )
                    .into_response()
            }
        },
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("token endpoint {status}: {body}")})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("token request failed: {e}")})),
            )
                .into_response()
        }
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
        // No userinfo endpoint — must verify id_token signature via JWKS before trusting
        // any claim. Per OIDC Core §3.1.3.7, skipping signature verification allows an
        // attacker to inject arbitrary org_id claims and bypass tenant isolation.
        let id_token_str =
            match token_resp.id_token.as_deref().filter(|t| !t.is_empty()) {
                Some(t) => t,
                None => return (
                    StatusCode::BAD_GATEWAY,
                    Json(
                        json!({"error": "provider returned no id_token and no userinfo endpoint"}),
                    ),
                )
                    .into_response(),
            };
        let jwks_uri = match disc.jwks_uri.as_deref().filter(|u| !u.is_empty()) {
            Some(u) => u.to_string(),
            None => return (StatusCode::BAD_GATEWAY, Json(json!({"error": "provider has no userinfo_endpoint and no jwks_uri — cannot verify id_token"}))).into_response(),
        };

        let header = match jsonwebtoken::decode_header(id_token_str) {
            Ok(h) => h,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("id_token header invalid: {e}")})),
                )
                    .into_response()
            }
        };

        let jwks: jsonwebtoken::jwk::JwkSet = match client.get(&jwks_uri).send().await {
            Ok(r) if r.status().is_success() => match r.json().await {
                Ok(j) => j,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({"error": format!("jwks parse failed: {e}")})),
                    )
                        .into_response()
                }
            },
            Ok(r) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("jwks fetch returned HTTP {}", r.status())})),
                )
                    .into_response()
            }
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("jwks fetch failed: {e}")})),
                )
                    .into_response()
            }
        };

        let kid = header.kid.as_deref().unwrap_or("");
        let jwk = match jwks.find(kid) {
            Some(j) => j,
            None => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": "no JWK matching id_token kid"})),
                )
                    .into_response()
            }
        };

        let decoding_key = match jsonwebtoken::DecodingKey::from_jwk(jwk) {
            Ok(k) => k,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("jwk key load failed: {e}")})),
                )
                    .into_response()
            }
        };

        let mut validation = jsonwebtoken::Validation::new(header.alg);
        validation.set_audience(&[cfg.client_id.as_str()]);
        validation.validate_exp = true;

        match jsonwebtoken::decode::<OidcUserInfo>(id_token_str, &decoding_key, &validation) {
            Ok(data) => data.claims,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("id_token verification failed: {e}")})),
                )
                    .into_response()
            }
        }
    };

    // Derive org_id from configured claim
    let org_id = if cfg.org_claim == "hd" {
        userinfo.hd.clone().unwrap_or_else(|| userinfo.sub.clone())
    } else {
        userinfo.sub.clone()
    };

    let sub = userinfo
        .email
        .as_deref()
        .unwrap_or(&userinfo.sub)
        .to_string();
    let session_claims = JwtClaims {
        sub,
        org_id,
        exp: (Utc::now() + Duration::hours(24)).timestamp() as usize,
    };
    let session_token = match sign_jwt(&session_claims, &jwt_secret) {
        Some(t) => t,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "session signing failed"})),
            )
                .into_response()
        }
    };

    let secure_flag = if cfg.redirect_uri.starts_with("https") {
        "; Secure"
    } else {
        ""
    };
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
pub(crate) async fn oidc_me(req: Request) -> Response {
    let jwt_secret = req
        .extensions()
        .get::<JwtSecretExt>()
        .and_then(|s| s.0.clone())
        .or_else(|| {
            std::env::var("RADAR_JWT_SECRET")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();
    if jwt_secret.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "auth not configured"})),
        )
            .into_response();
    }
    let cookie_header = req
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let token = parse_cookie(&cookie_header, "radar_session");
    match token.and_then(|t| validate_jwt(&t, &jwt_secret)) {
        Some(claims) => Json(json!({"sub": claims.sub, "org_id": claims.org_id})).into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "not authenticated"})),
        )
            .into_response(),
    }
}

/// GET /auth/logout — clear session cookie, redirect to /app/login.
pub(crate) async fn oidc_logout() -> Response {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(LOCATION, "/app/login".parse().unwrap());
    headers.insert(
        SET_COOKIE,
        "radar_session=; HttpOnly; SameSite=Lax; Max-Age=0; Path=/"
            .parse()
            .unwrap(),
    );
    (StatusCode::FOUND, headers).into_response()
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

/// Captured at router-build time so tests can't contaminate each other via env vars.
#[derive(Clone, Copy)]
pub(crate) struct RequireAuth(pub(crate) bool);

/// JWT secret injected at build time; falls back to RADAR_JWT_SECRET env var at runtime.
#[derive(Clone)]
pub(crate) struct JwtSecretExt(pub(crate) Option<String>);

pub(crate) async fn auth_middleware(
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
        .or_else(|| {
            std::env::var("RADAR_JWT_SECRET")
                .ok()
                .filter(|s| !s.is_empty())
        })
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
        let require_auth = req
            .extensions()
            .get::<RequireAuth>()
            .map(|r| r.0)
            .unwrap_or(false);
        if require_auth {
            drop(pool);
            return ApiError::Unauthorized.into_response();
        }
        return next.run(req).await;
    }

    let expected = format!("Bearer {service_token}");
    if !crate::utils::constant_time_eq(auth_header.as_bytes(), expected.as_bytes()) {
        drop(pool);
        return ApiError::Unauthorized.into_response();
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// C-2: Org-isolation guard — single canonical check for all handlers
// ---------------------------------------------------------------------------

/// Assert that `resource_org_id` matches `caller_org_id`. Returns `Forbidden` if not.
/// Skips the check when either ID is empty (anonymous or unauthenticated callers).
/// Use this in every handler that reads an org-owned resource to prevent cross-org access.
pub(crate) fn assert_org_access(
    resource_org_id: &str,
    caller_org_id: &str,
    resource_desc: &str,
) -> Result<(), ApiError> {
    if !caller_org_id.is_empty() && !resource_org_id.is_empty() && resource_org_id != caller_org_id
    {
        Err(ApiError::Forbidden(format!(
            "{resource_desc} belongs to another org"
        )))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// M-8: Shared cross-org ownership guard for handlers that take a resource id
// from the request path/body.
// ---------------------------------------------------------------------------

/// A fixed, allowlisted set of org-owned resource kinds. Each maps to a
/// hard-coded SQL statement that resolves the owning `org_id` from the
/// resource's primary-key id. Table and column names are compile-time
/// constants here — never interpolated from user input — so `require_org_owned`
/// cannot be coerced into reading an arbitrary table.
#[derive(Clone, Copy)]
pub(crate) enum OrgResource {
    /// `diff.id` → owning org via spec_version → service.
    Diff,
    /// `release_note.id` → owning org via diff → spec_version → service.
    ReleaseNote,
    /// `service.id` → `service.org_id`.
    Service,
    /// `consumer.id` → `consumer.org_id`.
    Consumer,
}

impl OrgResource {
    fn org_lookup_sql(self) -> &'static str {
        match self {
            OrgResource::Diff => {
                "SELECT s.org_id FROM diff d \
                 JOIN spec_version sv ON sv.id = d.from_version \
                 JOIN service s ON s.id = sv.service_id \
                 WHERE d.id = ?"
            }
            OrgResource::ReleaseNote => {
                "SELECT s.org_id FROM release_note rn \
                 JOIN diff d ON d.id = rn.diff_id \
                 JOIN spec_version sv ON sv.id = d.from_version \
                 JOIN service s ON s.id = sv.service_id \
                 WHERE rn.id = ?"
            }
            OrgResource::Service => "SELECT org_id FROM service WHERE id = ?",
            OrgResource::Consumer => "SELECT org_id FROM consumer WHERE id = ?",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            OrgResource::Diff => "diff",
            OrgResource::ReleaseNote => "release note",
            OrgResource::Service => "service",
            OrgResource::Consumer => "consumer",
        }
    }
}

/// Assert that the caller's org owns the resource identified by `id`.
///
/// Semantics (mirrors [`assert_org_access`] so the desktop/no-auth path keeps
/// working):
/// - Empty `caller_org_id` (desktop / single-tenant / unauthenticated) → `Ok`.
/// - Resource does not exist → `Ok` (the handler's own existence check produces
///   the `NotFound`; this guard never fabricates a 404).
/// - Resource's org is empty (row created in no-auth mode) → `Ok`.
/// - Non-empty caller org differs from non-empty resource org → `Forbidden` (403).
pub(crate) async fn require_org_owned(
    pool: &sqlx::AnyPool,
    resource: OrgResource,
    id: &str,
    caller_org_id: &str,
) -> Result<(), ApiError> {
    // Fast path: single-tenant / no-auth mode never triggers isolation.
    if caller_org_id.is_empty() {
        return Ok(());
    }
    let row = sqlx::query(resource.org_lookup_sql())
        .bind(id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => {
            use sqlx::Row;
            let resource_org: String = r
                .try_get::<Option<String>, _>(0)
                .unwrap_or_default()
                .unwrap_or_default();
            assert_org_access(&resource_org, caller_org_id, resource.desc())
        }
        None => Ok(()),
    }
}
