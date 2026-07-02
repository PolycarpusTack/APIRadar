use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::JwtClaims;
use crate::errors::ApiError;
use crate::utils::{is_host_allowed, is_ssrf_blocked, parse_codeowners};

const VALID_CATALOG_KINDS: &[&str] = &["backstage", "codeowners", "csv", "manual"];

/// Catalog sources may only read credentials from environment variables whose
/// name starts with this prefix. Prevents a caller from naming an arbitrary env
/// var (e.g. `RADAR_JWT_SECRET`, `ANTHROPIC_API_KEY`, `DATABASE_URL`) as the
/// bearer token and exfiltrating it to a client-supplied URL.
const CATALOG_TOKEN_ENV_PREFIX: &str = "RADAR_CATALOG_TOKEN_";

/// Validate that `token_env`, when set, references an allowlisted credential var.
fn token_env_allowed(token_env: Option<&str>) -> bool {
    match token_env {
        Some(env) if !env.is_empty() => env.starts_with(CATALOG_TOKEN_ENV_PREFIX),
        _ => true, // absent/empty is fine — no credential is read
    }
}

/// Pre-flight validation for a catalog source's outbound target, run before any
/// network call. Enforces (1) the token_env allowlist, (2) the SSRF guard
/// (HTTPS-only, no private/loopback/link-local addresses), and (3) the
/// `RADAR_ALLOWED_HOSTS` host allowlist. Returns Err(reason) if the target must
/// not be fetched.
fn validate_catalog_target(url: &str, token_env: Option<&str>) -> Result<(), String> {
    if !token_env_allowed(token_env) {
        return Err(format!(
            "token_env must reference a variable named {CATALOG_TOKEN_ENV_PREFIX}*"
        ));
    }
    if is_ssrf_blocked(url) {
        return Err("url blocked by SSRF guard (must be HTTPS to a public host)".to_string());
    }
    if !is_host_allowed(url) {
        return Err("url host is not in the RADAR_ALLOWED_HOSTS allowlist".to_string());
    }
    Ok(())
}

/// HTTP client for catalog sync: no redirect following (redirects can escape the
/// SSRF/host checks) and a bounded timeout so a hung endpoint can't stall a task.
fn catalog_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(serde::Deserialize)]
pub(crate) struct CreateCatalogSourceBody {
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) url: Option<String>,
    pub(crate) token_env: Option<String>,
    pub(crate) sync_interval_secs: Option<i64>,
}

/// POST /v1/catalog-sources — register a new catalog source configuration.
pub(crate) async fn create_catalog_source(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateCatalogSourceBody>,
) -> Result<impl IntoResponse, ApiError> {
    if !VALID_CATALOG_KINDS.contains(&body.kind.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "kind must be one of: {}",
            VALID_CATALOG_KINDS.join(", ")
        )));
    }
    if body.name.is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if !token_env_allowed(body.token_env.as_deref()) {
        return Err(ApiError::BadRequest(format!(
            "token_env must reference a variable named {CATALOG_TOKEN_ENV_PREFIX}*"
        )));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let url = body.url.unwrap_or_default();
    let interval = body.sync_interval_secs.unwrap_or(3600);

    sqlx::query(
        "INSERT INTO catalog_source (id, org_id, kind, name, url, token_env, sync_interval_secs, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(&body.kind)
    .bind(&body.name)
    .bind(&url)
    .bind(&body.token_env)
    .bind(interval)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":                 id,
            "org_id":             org_id,
            "kind":               body.kind,
            "name":               body.name,
            "url":                url,
            "token_env":          body.token_env,
            "sync_interval_secs": interval,
            "created_at":         now,
        })),
    ))
}

/// GET /v1/catalog-sources — list all catalog sources for the org.
pub(crate) async fn list_catalog_sources(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let rows = sqlx::query(
        "SELECT id, kind, name, url, token_env, sync_interval_secs, last_sync_at, last_sync_status, last_sync_error, created_at \
         FROM catalog_source WHERE org_id = ? ORDER BY created_at DESC",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            use sqlx::Row;
            json!({
                "id":                 r.get::<String, _>("id"),
                "kind":               r.get::<String, _>("kind"),
                "name":               r.get::<String, _>("name"),
                "url":                r.get::<String, _>("url"),
                "token_env":          r.get::<Option<String>, _>("token_env"),
                "sync_interval_secs": r.get::<i64, _>("sync_interval_secs"),
                "last_sync_at":       r.get::<Option<String>, _>("last_sync_at"),
                "last_sync_status":   r.get::<Option<String>, _>("last_sync_status"),
                "last_sync_error":    r.get::<Option<String>, _>("last_sync_error"),
                "created_at":         r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "entries": entries })))
}

/// POST /v1/catalog-sources/:id/sync — trigger an immediate sync for a catalog source.
pub(crate) async fn sync_catalog_source(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Path(source_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    let row = sqlx::query(
        "SELECT kind, url, token_env FROM catalog_source WHERE id = ? AND org_id = ?",
    )
    .bind(&source_id)
    .bind(&org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("catalog source {source_id} not found")))?;

    let kind: String = row.get("kind");
    let url: String = row.get("url");
    let token_env: Option<String> = row.get("token_env");

    // Refuse to fetch — and to read any credential — unless the target passes the
    // token_env allowlist, SSRF guard, and host allowlist.
    let (upserted, error_msg) = match validate_catalog_target(&url, token_env.as_deref()) {
        Err(reason) => (0, Some(reason)),
        Ok(()) => {
            let token = token_env
                .as_deref()
                .and_then(|env| std::env::var(env).ok());
            match kind.as_str() {
                "backstage"  => sync_backstage_source(&pool, &org_id, &url, token.as_deref()).await,
                "codeowners" => sync_codeowners_source(&pool, &org_id, &url, token.as_deref()).await,
                _            => (0, Some(format!("sync not implemented for kind={kind}"))),
            }
        }
    };

    let status = if error_msg.is_none() { "ok" } else { "error" };

    sqlx::query(
        "UPDATE catalog_source SET last_sync_at = ?, last_sync_status = ?, last_sync_error = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(status)
    .bind(&error_msg)
    .bind(&source_id)
    .execute(&pool)
    .await?;

    Ok(Json(json!({
        "source_id":          source_id,
        "synced_at":          now,
        "status":             status,
        "consumers_upserted": upserted,
        "error":              error_msg,
    })))
}

/// Fetch a CODEOWNERS file from `url` and upsert one Consumer per unique owner.
/// Returns (upserted_count, Option<error_message>).
async fn sync_codeowners_source(
    pool: &sqlx::AnyPool,
    org_id: &str,
    url: &str,
    token: Option<&str>,
) -> (usize, Option<String>) {
    let mut req = catalog_http_client().get(url);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }

    let resp = match req.send().await {
        Ok(r)  => r,
        Err(e) => return (0, Some(format!("HTTP error fetching CODEOWNERS: {e}"))),
    };
    if !resp.status().is_success() {
        let status = resp.status();
        return (0, Some(format!("CODEOWNERS fetch returned {status}")));
    }

    let content = match resp.text().await {
        Ok(t)  => t,
        Err(e) => return (0, Some(format!("Failed to read CODEOWNERS body: {e}"))),
    };

    let owners = parse_codeowners(&content);
    let now = Utc::now().to_rfc3339();
    let mut upserted = 0usize;

    for owner in &owners {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM consumer WHERE org_id = ? AND name = ? LIMIT 1",
        )
        .bind(org_id)
        .bind(owner)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        if let Some(existing_id) = existing {
            let _ = sqlx::query("UPDATE consumer SET catalog_source = ? WHERE id = ?")
                .bind("codeowners")
                .bind(&existing_id)
                .execute(pool)
                .await;
        } else {
            let _ = sqlx::query(
                "INSERT INTO consumer (id, org_id, name, repo_url, owner_team, contact, catalog_source, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(org_id)
            .bind(owner)
            .bind("")
            .bind(owner)
            .bind("")
            .bind("codeowners")
            .bind(&now)
            .execute(pool)
            .await;
        }
        upserted += 1;
    }

    (upserted, None)
}

/// Fetch Backstage catalog Component entities and upsert them as consumers.
/// Returns (upserted_count, Option<error_message>).
async fn sync_backstage_source(
    pool: &sqlx::AnyPool,
    org_id: &str,
    base_url: &str,
    token: Option<&str>,
) -> (usize, Option<String>) {
    let url = format!(
        "{}/api/catalog/entities?filter=kind%3DComponent&limit=500",
        base_url.trim_end_matches('/')
    );

    let mut req = catalog_http_client().get(&url);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }

    let resp = match req.send().await {
        Ok(r)  => r,
        Err(e) => return (0, Some(format!("HTTP error fetching Backstage entities: {e}"))),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        return (0, Some(format!("Backstage API returned {status}")));
    }

    let entities: serde_json::Value = match resp.json().await {
        Ok(v)  => v,
        Err(e) => return (0, Some(format!("Failed to parse Backstage response: {e}"))),
    };

    let items = match entities.as_array() {
        Some(a) => a,
        None    => return (0, Some("Backstage response was not an array".to_string())),
    };

    let mut upserted = 0usize;
    let now = Utc::now().to_rfc3339();

    for item in items {
        let name = match item["metadata"]["name"].as_str() {
            Some(n) if !n.is_empty() => n.to_string(),
            _                        => continue,
        };
        let owner = item["spec"]["owner"].as_str().unwrap_or("").to_string();

        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM consumer WHERE org_id = ? AND name = ? LIMIT 1",
        )
        .bind(org_id)
        .bind(&name)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        if let Some(existing_id) = existing {
            let _ = sqlx::query(
                "UPDATE consumer SET owner_team = ?, catalog_source = ? WHERE id = ?",
            )
            .bind(&owner)
            .bind("backstage")
            .bind(&existing_id)
            .execute(pool)
            .await;
        } else {
            let consumer_id = Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO consumer (id, org_id, name, repo_url, owner_team, contact, catalog_source, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&consumer_id)
            .bind(org_id)
            .bind(&name)
            .bind("")
            .bind(&owner)
            .bind("")
            .bind("backstage")
            .bind(&now)
            .execute(pool)
            .await;
        }
        upserted += 1;
    }

    (upserted, None)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_env_off_allowlist_is_rejected() {
        // Arbitrary secret var names must be refused.
        assert!(!token_env_allowed(Some("RADAR_JWT_SECRET")));
        assert!(!token_env_allowed(Some("ANTHROPIC_API_KEY")));
        assert!(!token_env_allowed(Some("DATABASE_URL")));
        assert!(!token_env_allowed(Some("GITHUB_TOKEN")));
    }

    #[test]
    fn token_env_on_allowlist_or_absent_is_allowed() {
        assert!(token_env_allowed(Some("RADAR_CATALOG_TOKEN_BACKSTAGE")));
        assert!(token_env_allowed(Some("RADAR_CATALOG_TOKEN_")));
        assert!(token_env_allowed(None));
        assert!(token_env_allowed(Some("")));
    }

    #[test]
    fn validate_rejects_off_allowlist_token_env() {
        // Even a perfectly good HTTPS public URL must be refused if it would read
        // a non-allowlisted credential var.
        let err = validate_catalog_target("https://example.com/catalog", Some("RADAR_JWT_SECRET"));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("token_env"));
    }

    #[test]
    fn validate_blocks_ssrf_targets() {
        // Private / loopback / non-HTTPS targets are blocked before any fetch.
        assert!(validate_catalog_target("https://169.254.169.254/latest/meta-data/", None).is_err());
        assert!(validate_catalog_target("https://127.0.0.1/catalog", None).is_err());
        assert!(validate_catalog_target("https://10.0.0.1/catalog", None).is_err());
        assert!(validate_catalog_target("http://example.com/catalog", None).is_err()); // non-HTTPS
    }

    #[test]
    fn validate_allows_public_https_with_allowlisted_token() {
        // Use a public IP literal so the check is hermetic (no DNS needed): a
        // public, non-private address with an allowlisted token_env must pass.
        std::env::remove_var("RADAR_ALLOWED_HOSTS");
        assert!(validate_catalog_target(
            "https://93.184.216.34/api/catalog",
            Some("RADAR_CATALOG_TOKEN_BACKSTAGE"),
        )
        .is_ok());
    }
}
