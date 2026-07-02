// ---------------------------------------------------------------------------
// Scalar bundle: runtime version check + disk-based override
// ---------------------------------------------------------------------------
//
// The Scalar standalone JS bundle is compiled into the binary via include_bytes!.
// In desktop (SQLite) mode, a newer version can be downloaded from the CDN and
// stored as an override file alongside the database.  The override is loaded at
// request time — no restart required to switch.
//
// Override files (written by `post_update`):
//   <db-dir>/scalar_override.js       — newer JS bundle
//   <db-dir>/scalar_override.version  — version string (e.g. "1.58.0")
//
// OVERRIDE_DIR is set once by `run()` before the server begins serving requests.
// Tests never call `run()`, so OVERRIDE_DIR remains unset and the compiled-in
// bundle is always served — which is correct for unit tests.

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Global override directory
// ---------------------------------------------------------------------------

/// Set once by `radar_api::run()` immediately after resolving the database URL.
/// `None` means no disk-based override is supported (PostgreSQL, in-memory).
pub(crate) static OVERRIDE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

// ---------------------------------------------------------------------------
// Compiled-in bundle
// ---------------------------------------------------------------------------

static BUNDLED_JS: &[u8] = include_bytes!("../vendor/scalar.js");
static BUNDLED_VERSION: &str = include_str!("../vendor/scalar.version");

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Returns the active JS bytes and a flag indicating whether they came from the
/// compiled-in bundle (`true`) or from a disk override (`false`).
pub(crate) fn active_js() -> (Vec<u8>, bool) {
    if let Some(Some(dir)) = OVERRIDE_DIR.get() {
        let path = dir.join("scalar_override.js");
        if let Ok(bytes) = std::fs::read(&path) {
            return (bytes, false);
        }
    }
    (BUNDLED_JS.to_vec(), true)
}

/// Returns the active version string.
pub(crate) fn active_version() -> String {
    if let Some(Some(dir)) = OVERRIDE_DIR.get() {
        let path = dir.join("scalar_override.version");
        if let Ok(v) = std::fs::read_to_string(&path) {
            let trimmed = v.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    BUNDLED_VERSION.trim().to_string()
}

fn override_version() -> Option<String> {
    let dir = OVERRIDE_DIR.get()?.as_ref()?;
    let path = dir.join("scalar_override.version");
    let v = std::fs::read_to_string(path).ok()?;
    let trimmed = v.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub(crate) struct VersionResponse {
    bundled: String,
    #[serde(rename = "override")]
    override_ver: Option<String>,
    active: String,
    latest: Option<String>,
    update_available: bool,
}

/// GET /scalar/version
/// Returns the bundled, override, and latest-on-npm versions.
pub(crate) async fn get_version() -> impl IntoResponse {
    let bundled = BUNDLED_VERSION.trim().to_string();
    let override_ver = override_version();
    let active = active_version();
    let latest = fetch_latest_npm_version().await;

    let update_available = match &latest {
        Some(v) => is_newer(v, &active),
        None => false,
    };

    Json(VersionResponse {
        bundled,
        override_ver,
        active,
        latest,
        update_available,
    })
}

/// POST /scalar/update
/// Downloads the latest Scalar bundle from jsDelivr and stores it as the disk override.
/// Only works in SQLite (desktop) mode; returns 400 for PostgreSQL deployments.
/// Requires a valid Bearer token when either RADAR_JWT_SECRET or RADAR_SERVICE_TOKEN is set.
pub(crate) async fn post_update(headers: axum::http::HeaderMap) -> impl IntoResponse {
    // Require Bearer token if either auth mechanism is configured.
    let jwt_secret = std::env::var("RADAR_JWT_SECRET").unwrap_or_default();
    let service_token = std::env::var("RADAR_SERVICE_TOKEN").unwrap_or_default();
    if !jwt_secret.is_empty() || !service_token.is_empty() {
        let auth = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let ok = if !jwt_secret.is_empty() {
            auth.strip_prefix("Bearer ")
                .and_then(|t| crate::auth::validate_jwt(t, &jwt_secret))
                .is_some()
        } else {
            crate::utils::constant_time_eq(
                auth.as_bytes(),
                format!("Bearer {service_token}").as_bytes(),
            )
        };
        if !ok {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({"error": "unauthorized"})),
            )
                .into_response();
        }
    }

    let override_dir = match OVERRIDE_DIR.get() {
        Some(Some(dir)) => dir.clone(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Scalar updates are only supported in SQLite (local / desktop) mode."
                })),
            )
                .into_response();
        }
    };

    let latest = match fetch_latest_npm_version().await {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": "Could not determine the latest Scalar version from the npm registry. Check your internet connection."
                })),
            )
                .into_response();
        }
    };

    let cdn_url = format!(
        "https://cdn.jsdelivr.net/npm/@scalar/api-reference@{latest}/dist/browser/standalone.js"
    );

    let bytes = match download_bytes(&cdn_url).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("Download failed: {e}") })),
            )
                .into_response();
        }
    };

    let js_path = override_dir.join("scalar_override.js");
    let ver_path = override_dir.join("scalar_override.version");

    if let Err(e) = std::fs::write(&js_path, &bytes) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to write override bundle: {e}") })),
        )
            .into_response();
    }
    if let Err(e) = std::fs::write(&ver_path, &latest) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("Failed to write override version file: {e}") })),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "updated": true,
            "version": latest,
            "bytes": bytes.len()
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn fetch_latest_npm_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let resp = client
        .get("https://registry.npmjs.org/@scalar/api-reference/latest")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    body["version"].as_str().map(|s| s.to_string())
}

async fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

/// Simple semantic version comparison.  Returns `true` iff `candidate` > `current`.
/// Handles three-part versions like "1.57.5"; pre-release tags are stripped.
fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() < 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            // Strip any pre-release suffix (e.g. "5-alpha.1" → "5")
            parts[2].split('-').next().unwrap_or(parts[2]).parse().ok()?,
        ))
    }
    match (parse(candidate), parse(current)) {
        (Some(c), Some(a)) => c > a,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn is_newer_works() {
        assert!(is_newer("1.58.0", "1.57.5"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.57.5", "1.57.5")); // same → not newer
        assert!(!is_newer("1.57.4", "1.57.5")); // older
        assert!(is_newer("1.57.6-beta.1", "1.57.5")); // pre-release still increments patch
    }
}
