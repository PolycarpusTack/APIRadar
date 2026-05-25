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
use crate::utils::parse_codeowners;

const VALID_CATALOG_KINDS: &[&str] = &["backstage", "codeowners", "csv", "manual"];

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

    let token = token_env
        .as_deref()
        .and_then(|env| std::env::var(env).ok());

    let (upserted, error_msg) = match kind.as_str() {
        "backstage"  => sync_backstage_source(&pool, &org_id, &url, token.as_deref()).await,
        "codeowners" => sync_codeowners_source(&pool, &org_id, &url, token.as_deref()).await,
        _            => (0, Some(format!("sync not implemented for kind={kind}"))),
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
    let mut req = reqwest::Client::new().get(url);
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

    let mut req = reqwest::Client::new().get(&url);
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
