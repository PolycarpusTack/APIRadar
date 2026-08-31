use crate::errors::ApiError;
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::{
    extract::{Query, Request},
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

/// Domain-separation labels. Session tokens and short-lived OIDC CSRF-state
/// tokens are both HS256 and both derived from `RADAR_JWT_SECRET`; deriving a
/// distinct key per purpose means a token minted for one can never verify as
/// the other, no matter how the claim structs later evolve. Previously the two
/// were only kept apart by which fields serde happened to require — an
/// accidental property that a single `#[serde(default)]` would have removed.
const KEY_DOMAIN_SESSION: &str = "radar.session.v1";
const KEY_DOMAIN_OIDC_STATE: &str = "radar.oidc-state.v1";

/// Derive a purpose-specific signing key from the configured secret.
fn derive_key(secret: &str, domain: &str) -> Vec<u8> {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts a key of any length");
    mac.update(domain.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Validate an HS256 session JWT using RADAR_JWT_SECRET. Returns claims on success.
pub(crate) fn validate_jwt(token: &str, secret: &str) -> Option<JwtClaims> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    let key = DecodingKey::from_secret(&derive_key(secret, KEY_DOMAIN_SESSION));
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
    /// REQUIRED by OIDC Discovery §3. Used to pin the `iss` claim when
    /// verifying an id_token, so a token minted by a different provider that
    /// happens to share a JWKS host cannot be replayed at us.
    #[serde(default)]
    issuer: Option<String>,
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
    /// Present only when these claims came from an id_token. Must equal the
    /// nonce we put in the authorization request (OIDC Core §3.1.3.7), which
    /// is what makes an intercepted id_token non-replayable.
    #[serde(default)]
    nonce: Option<String>,
}

/// Short-lived CSRF state token embedded as a signed JWT.
#[derive(serde::Serialize, serde::Deserialize)]
struct OidcState {
    nonce: String,
    exp: usize,
}

/// Derive the tenant `org_id` from the identity provider's claims.
///
/// Returns `None` when the provider supplied nothing usable. That distinction
/// is a security boundary, not a convenience: an empty `org_id` is this
/// system's "no isolation" wildcard — [`require_org_owned`] short-circuits to
/// `Ok` on it, [`assert_org_access`] skips its check, and the reporting
/// queries spell it out as `(? = '' OR s.org_id = ?)`. A session carrying an
/// empty org therefore reads *every* tenant's data, so callers must reject the
/// login on `None` rather than substituting a default.
fn derive_org_id(userinfo: &OidcUserInfo, org_claim: &str) -> Option<String> {
    let candidate = if org_claim == "hd" {
        userinfo.hd.clone().unwrap_or_else(|| userinfo.sub.clone())
    } else {
        userinfo.sub.clone()
    };
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Sign a JwtClaims struct into an HS256 JWT string.
pub(crate) fn sign_jwt(claims: &JwtClaims, secret: &str) -> Option<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(&derive_key(secret, KEY_DOMAIN_SESSION)),
    )
    .ok()
}

/// Sign an OidcState into an HS256 JWT string.
fn sign_state(state: &OidcState, secret: &str) -> Option<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    encode(
        &Header::new(Algorithm::HS256),
        state,
        &EncodingKey::from_secret(&derive_key(secret, KEY_DOMAIN_OIDC_STATE)),
    )
    .ok()
}

/// Validate an OidcState JWT and return the nonce if valid.
fn validate_state(token: &str, secret: &str) -> Option<String> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    let key = DecodingKey::from_secret(&derive_key(secret, KEY_DOMAIN_OIDC_STATE));
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
        "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid+email+profile&state={}&nonce={}",
        disc.authorization_endpoint,
        urlencoding_encode(&cfg.client_id),
        urlencoding_encode(&cfg.redirect_uri),
        urlencoding_encode(&state_token),
        urlencoding_encode(&nonce),
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
    let expected_nonce = validate_state(&state_param, &jwt_secret);
    if state_cookie_val.as_deref() != Some(state_param.as_str()) || expected_nonce.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid or expired state"})),
        )
            .into_response();
    }
    // Safe: the `is_none()` arm above already returned.
    let expected_nonce = expected_nonce.unwrap_or_default();

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
        // A failed userinfo call must abort the login. Defaulting here would
        // yield `sub: ""` / `hd: None`, which derives an EMPTY org_id — and an
        // empty org_id is this system's "no isolation" wildcard, so a provider
        // hiccup (or a 403 from a declined `profile` scope) would mint a
        // session able to read every org's data.
        let resp = match client
            .get(&userinfo_url)
            .bearer_auth(&token_resp.access_token)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("userinfo request failed: {e}")})),
                )
                    .into_response()
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("userinfo endpoint returned HTTP {status}")})),
            )
                .into_response();
        }
        match resp.json::<OidcUserInfo>().await {
            Ok(u) => u,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("userinfo parse failed: {e}")})),
                )
                    .into_response()
            }
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

        // F-04: never take the algorithm from the token header — that is the
        // shape of the classic alg-confusion bug. Pin the asymmetric algorithms
        // an OIDC provider is allowed to use; anything symmetric (HS*) would
        // mean the "signature" is verified with a value the token influences.
        const ALLOWED_ID_TOKEN_ALGS: &[jsonwebtoken::Algorithm] = &[
            jsonwebtoken::Algorithm::RS256,
            jsonwebtoken::Algorithm::RS384,
            jsonwebtoken::Algorithm::RS512,
            jsonwebtoken::Algorithm::PS256,
            jsonwebtoken::Algorithm::PS384,
            jsonwebtoken::Algorithm::PS512,
            jsonwebtoken::Algorithm::ES256,
            jsonwebtoken::Algorithm::ES384,
        ];
        if !ALLOWED_ID_TOKEN_ALGS.contains(&header.alg) {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({"error": format!("id_token algorithm {:?} is not permitted", header.alg)}),
                ),
            )
                .into_response();
        }

        // F-06: pin the issuer from discovery. Verifying an id_token without
        // checking who minted it accepts any token signed by any key in the
        // JWKS we happened to fetch.
        let Some(issuer) = disc.issuer.as_deref().filter(|s| !s.is_empty()) else {
            return (
                StatusCode::BAD_GATEWAY,
                Json(
                    json!({"error": "provider discovery omitted `issuer`; cannot verify id_token"}),
                ),
            )
                .into_response();
        };

        let mut validation = jsonwebtoken::Validation::new(header.alg);
        validation.algorithms = ALLOWED_ID_TOKEN_ALGS.to_vec();
        validation.set_audience(&[cfg.client_id.as_str()]);
        validation.set_issuer(&[issuer]);
        validation.validate_exp = true;

        let claims: OidcUserInfo =
            match jsonwebtoken::decode::<OidcUserInfo>(id_token_str, &decoding_key, &validation) {
                Ok(data) => data.claims,
                Err(e) => {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(json!({"error": format!("id_token verification failed: {e}")})),
                    )
                        .into_response()
                }
            };

        // F-05: bind the id_token to this authorization request.
        if claims.nonce.as_deref() != Some(expected_nonce.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "id_token nonce does not match the authorization request"})),
            )
                .into_response();
        }
        claims
    };

    // Derive org_id from the configured claim. `None` means the provider gave
    // us nothing usable — fail the login rather than mint a cross-tenant
    // session (see `derive_org_id`).
    let Some(org_id) = derive_org_id(&userinfo, &cfg.org_claim) else {
        tracing::warn!("OIDC login rejected: provider returned no usable org/sub claim");
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "identity provider returned no usable org claim"})),
        )
            .into_response();
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

    // F-14: behind a TLS-terminating proxy the redirect_uri is often plain
    // http even though the browser connection is https, and inferring the
    // flag from it silently drops `Secure` exactly where it matters most.
    // RADAR_COOKIE_SECURE settles it explicitly; the old heuristic remains the
    // default so local http development keeps working untouched.
    let secure_flag = match std::env::var("RADAR_COOKIE_SECURE") {
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true") => "; Secure",
        Ok(v) if v == "0" || v.eq_ignore_ascii_case("false") => "",
        _ if cfg.redirect_uri.starts_with("https") => "; Secure",
        _ => "",
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

pub(crate) async fn auth_middleware(mut req: Request, next: Next) -> Response {
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
            return ApiError::Unauthorized.into_response();
        }
        return next.run(req).await;
    }

    let expected = format!("Bearer {service_token}");
    if !crate::utils::constant_time_eq(auth_header.as_bytes(), expected.as_bytes()) {
        return ApiError::Unauthorized.into_response();
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// F-02: the caller's org, in a form that cannot silently become a wildcard
// ---------------------------------------------------------------------------

/// Whether this server was started without any tenant concept.
///
/// Decided once at router-build time from the auth configuration, never read
/// from the environment per request. `true` only when no authentication is
/// configured at all — desktop and single-tenant deployments.
#[derive(Clone, Copy)]
pub(crate) struct SingleTenantMode(pub(crate) bool);

/// Who is asking.
///
/// This type exists because the previous representation — a bare `String`
/// obtained with `org.map(..).unwrap_or_default()` — made the dangerous value
/// the *easiest* one to produce. An absent or malformed claim yielded `""`,
/// and `""` is this system's "every org" wildcard: `require_org_owned`
/// short-circuits on it, `assert_org_access` skips its check, and the
/// reporting queries read `(? = '' OR s.org_id = ?)`. F-01 was exactly that
/// bug, reached through a single `unwrap_or_default()`.
///
/// Here the wildcard has its own variant, and that variant is only reachable
/// when the server is explicitly in single-tenant mode. In a multi-tenant
/// deployment a missing claim is an error, not an empty string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CallerOrg {
    /// A specific tenant. Every org-scoped query must filter by this.
    Tenant(String),
    /// No tenant concept on this server, so isolation is meaningless and the
    /// scope is deliberately unrestricted.
    SingleTenant,
}

impl CallerOrg {
    /// Resolve the caller's org, or `None` when the request must be rejected.
    ///
    /// Returning `None` rather than an empty string is the whole point: there
    /// is no value a caller can end up with that quietly means "everything".
    pub(crate) fn resolve(claims: Option<&JwtClaims>, mode: SingleTenantMode) -> Option<Self> {
        match claims {
            // A signed claim with a usable org.
            Some(c) if !c.org_id.trim().is_empty() => {
                Some(CallerOrg::Tenant(c.org_id.trim().to_string()))
            }
            // A signed claim with an empty org should be impossible since
            // F-01, because the login refuses to mint one. Treat it as hostile
            // rather than as the wildcard it used to become.
            Some(_) => None,
            // No claims: legitimate only when there are no tenants at all.
            None if mode.0 => Some(CallerOrg::SingleTenant),
            None => None,
        }
    }

    /// The value to bind into an org-scoped `WHERE` clause.
    ///
    /// `SingleTenant` yields `""`, which the `(? = '' OR org_id = ?)` guards
    /// read as "no filter". That is still a wildcard — but it is now only
    /// produced by a variant that cannot exist unless the operator configured
    /// a server with no tenants.
    pub(crate) fn sql_scope(&self) -> &str {
        match self {
            CallerOrg::Tenant(id) => id,
            CallerOrg::SingleTenant => "",
        }
    }
}

/// Extracting `CallerOrg` directly is what makes F-02 a compile-time property
/// rather than a convention: a handler cannot obtain an org string without
/// going through [`CallerOrg::resolve`], and a request that resolves to
/// nothing is rejected here rather than silently scoped to every tenant.
#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for CallerOrg
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts.extensions.get::<JwtClaims>();
        // Absent extension → assume multi-tenant. Failing closed matters more
        // than convenience if the layer is ever mis-ordered.
        let mode = parts
            .extensions
            .get::<SingleTenantMode>()
            .copied()
            .unwrap_or(SingleTenantMode(false));
        CallerOrg::resolve(claims, mode).ok_or(ApiError::Unauthorized)
    }
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
    let row = q!(resource.org_lookup_sql())
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

// ---------------------------------------------------------------------------
// F-01 regression: the OIDC callback must never mint an empty-org session.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug: `userinfo` failures used to fall back to `OidcUserInfo::default()`,
    /// which derives an EMPTY org_id — and an empty org_id disables tenant
    /// isolation across the whole API. The default value must never yield a
    /// usable org.
    #[test]
    fn default_userinfo_yields_no_org() {
        let empty = OidcUserInfo::default();
        assert_eq!(
            derive_org_id(&empty, "hd"),
            None,
            "a defaulted userinfo must not produce an org_id — an empty org_id \
             is the cross-tenant wildcard"
        );
        assert_eq!(derive_org_id(&empty, "sub"), None);
    }

    /// Whitespace-only claims are just as dangerous as empty ones once trimmed.
    #[test]
    fn blank_claims_yield_no_org() {
        let blank = OidcUserInfo {
            sub: "   ".to_string(),
            hd: Some("\t".to_string()),
            ..Default::default()
        };
        assert_eq!(derive_org_id(&blank, "hd"), None);
        assert_eq!(derive_org_id(&blank, "sub"), None);
    }

    #[test]
    fn hd_claim_is_preferred_when_configured() {
        let u = OidcUserInfo {
            sub: "user-123".to_string(),
            hd: Some("acme.example".to_string()),
            ..Default::default()
        };
        assert_eq!(derive_org_id(&u, "hd"), Some("acme.example".to_string()));
    }

    #[test]
    fn hd_falls_back_to_sub_when_absent() {
        let u = OidcUserInfo {
            sub: "user-123".to_string(),
            hd: None,
            ..Default::default()
        };
        assert_eq!(derive_org_id(&u, "hd"), Some("user-123".to_string()));
    }

    #[test]
    fn non_hd_claim_uses_sub_and_ignores_hd() {
        let u = OidcUserInfo {
            sub: "user-123".to_string(),
            hd: Some("acme.example".to_string()),
            ..Default::default()
        };
        assert_eq!(derive_org_id(&u, "sub"), Some("user-123".to_string()));
    }

    // ---- F-02: the caller's org can never silently become a wildcard ----

    fn claims_with(org: &str) -> JwtClaims {
        JwtClaims {
            sub: "user@example.com".into(),
            org_id: org.into(),
            exp: 9_999_999_999,
        }
    }

    const MULTI: SingleTenantMode = SingleTenantMode(false);
    const SINGLE: SingleTenantMode = SingleTenantMode(true);

    /// The F-01 shape: claims that carry no usable org must be rejected, not
    /// converted into the empty string that means "every tenant".
    #[test]
    fn empty_org_claim_is_rejected_not_widened() {
        assert_eq!(CallerOrg::resolve(Some(&claims_with("")), MULTI), None);
        assert_eq!(CallerOrg::resolve(Some(&claims_with("   ")), MULTI), None);
        // ...and the same in single-tenant mode: a claim that exists but is
        // unusable is hostile regardless of how the server is configured.
        assert_eq!(CallerOrg::resolve(Some(&claims_with("")), SINGLE), None);
    }

    /// Missing claims in a multi-tenant deployment are an authentication
    /// failure. Previously this produced "" via unwrap_or_default().
    #[test]
    fn missing_claims_are_rejected_when_tenants_exist() {
        assert_eq!(CallerOrg::resolve(None, MULTI), None);
    }

    #[test]
    fn missing_claims_are_allowed_only_in_single_tenant_mode() {
        assert_eq!(
            CallerOrg::resolve(None, SINGLE),
            Some(CallerOrg::SingleTenant)
        );
    }

    #[test]
    fn a_real_org_resolves_to_that_tenant() {
        assert_eq!(
            CallerOrg::resolve(Some(&claims_with("acme.example")), MULTI),
            Some(CallerOrg::Tenant("acme.example".into()))
        );
        // Surrounding whitespace is trimmed rather than producing a distinct
        // tenant that matches no rows.
        assert_eq!(
            CallerOrg::resolve(Some(&claims_with("  acme.example  ")), MULTI),
            Some(CallerOrg::Tenant("acme.example".into()))
        );
    }

    #[test]
    fn only_the_single_tenant_variant_yields_an_unrestricted_scope() {
        assert_eq!(CallerOrg::SingleTenant.sql_scope(), "");
        assert_eq!(
            CallerOrg::Tenant("acme.example".into()).sql_scope(),
            "acme.example"
        );
    }

    // ---- Batch A: token domain separation (F-10) ----

    const T_SECRET: &str = "batch-a-test-secret";

    #[test]
    fn key_domains_produce_distinct_keys() {
        assert_ne!(
            derive_key(T_SECRET, KEY_DOMAIN_SESSION),
            derive_key(T_SECRET, KEY_DOMAIN_OIDC_STATE),
            "session and oidc-state keys must differ, or a token minted for one \
             purpose can verify as the other"
        );
    }

    #[test]
    fn state_token_does_not_validate_as_session() {
        let state = OidcState {
            nonce: "n-1".into(),
            exp: 9_999_999_999,
        };
        let tok = sign_state(&state, T_SECRET).expect("sign_state");
        assert!(
            validate_jwt(&tok, T_SECRET).is_none(),
            "a CSRF-state token must never be accepted as a session"
        );
    }

    #[test]
    fn session_token_does_not_validate_as_state() {
        let claims = JwtClaims {
            sub: "user@example.com".into(),
            org_id: "acme.example".into(),
            exp: 9_999_999_999,
        };
        let tok = sign_jwt(&claims, T_SECRET).expect("sign_jwt");
        assert!(
            validate_state(&tok, T_SECRET).is_none(),
            "a session token must never be accepted as CSRF state"
        );
    }

    #[test]
    fn session_token_round_trips() {
        let claims = JwtClaims {
            sub: "user@example.com".into(),
            org_id: "acme.example".into(),
            exp: 9_999_999_999,
        };
        let tok = sign_jwt(&claims, T_SECRET).expect("sign_jwt");
        let back = validate_jwt(&tok, T_SECRET).expect("round trip");
        assert_eq!(back.org_id, "acme.example");
        assert_eq!(back.sub, "user@example.com");
    }

    #[test]
    fn state_nonce_round_trips() {
        let state = OidcState {
            nonce: "nonce-abc".into(),
            exp: 9_999_999_999,
        };
        let tok = sign_state(&state, T_SECRET).expect("sign_state");
        assert_eq!(validate_state(&tok, T_SECRET).as_deref(), Some("nonce-abc"));
    }

    #[test]
    fn tokens_do_not_validate_under_a_different_secret() {
        let claims = JwtClaims {
            sub: "u".into(),
            org_id: "o".into(),
            exp: 9_999_999_999,
        };
        let tok = sign_jwt(&claims, T_SECRET).expect("sign_jwt");
        assert!(validate_jwt(&tok, "a-different-secret").is_none());
    }

    /// Why the old `r.json().await.unwrap_or_default()` was exploitable: an
    /// OAuth error body does not carry `sub`, so deserialization fails and the
    /// old code silently substituted the all-access default. This test pins the
    /// fact that such a body is NOT a valid identity.
    #[test]
    fn oauth_error_body_is_not_a_valid_identity() {
        let err_body = r#"{"error":"insufficient_scope","error_description":"missing profile"}"#;
        let parsed: Result<OidcUserInfo, _> = serde_json::from_str(err_body);
        assert!(
            parsed.is_err(),
            "an OAuth error body must not deserialize into an identity"
        );
    }
}
