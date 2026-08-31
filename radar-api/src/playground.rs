use crate::auth::{assert_org_access, CallerOrg};
use crate::errors::ApiError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

#[derive(serde::Deserialize)]
pub(crate) struct SandboxEnvBody {
    name: String,
    #[serde(default)]
    base_url: String,
    // None = keep existing token; Some("") = clear token; Some(t) = update token.
    #[serde(default)]
    bearer_token: Option<String>,
    #[serde(default)]
    description: String,
}

fn mask_token(t: &str) -> String {
    // Count characters (not bytes) and slice on a char boundary so a multi-byte
    // UTF-8 token (e.g. "€€€€€") never panics. Mirrors webhooks::mask_secret.
    let char_count = t.chars().count();
    if char_count <= 4 {
        "***".into()
    } else {
        let start = t
            .char_indices()
            .nth(char_count - 4)
            .map(|(i, _)| i)
            .unwrap_or(t.len());
        format!("***{}", &t[start..])
    }
}

// GET /v1/sandbox-envs
pub(crate) async fn list_sandbox_envs(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let org_id = caller.sql_scope().to_string();

    let rows = q!(
        "SELECT id, name, base_url, bearer_token, description, created_at, updated_at \
         FROM sandbox_env WHERE org_id = ? ORDER BY name ASC",
    )
    .bind(&org_id)
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
                // Return the masked hint for display; never a value the UI should send back.
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
pub(crate) async fn create_sandbox_env(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Json(body): Json<SandboxEnvBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::Unprocessable("name is required".into()));
    }

    let org_id = caller.sql_scope().to_string();
    let token = body.bearer_token.as_deref().unwrap_or("");
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    q!("INSERT INTO sandbox_env \
         (id, org_id, name, base_url, bearer_token, description, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",)
    .bind(&id)
    .bind(&org_id)
    .bind(body.name.trim())
    .bind(body.base_url.trim())
    .bind(token)
    .bind(body.description.trim())
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": body.name.trim(),
            "base_url": body.base_url.trim(),
            "bearer_token": mask_token(token),
            "bearer_token_set": !token.is_empty(),
            "description": body.description.trim(),
            "created_at": now,
            "updated_at": now,
        })),
    ))
}

// PUT /v1/sandbox-envs/:id
pub(crate) async fn update_sandbox_env(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Path(id): Path<String>,
    Json(body): Json<SandboxEnvBody>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    if body.name.trim().is_empty() {
        return Err(ApiError::Unprocessable("name is required".into()));
    }

    let org_id = caller.sql_scope().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Verify ownership and fetch the current token so we can preserve it when
    // the caller doesn't provide a new one.
    let existing = q!("SELECT bearer_token FROM sandbox_env WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("sandbox environment not found".into()))?;

    let current_token: String = existing.try_get("bearer_token").unwrap_or_default();
    // Use new token only when the caller supplies a non-empty string.
    // Empty string (UI left the field blank) or omitted field both mean "keep existing".
    let new_token = match body.bearer_token.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => current_token.clone(),
    };

    q!("UPDATE sandbox_env \
         SET name = ?, base_url = ?, bearer_token = ?, description = ?, updated_at = ? \
         WHERE id = ? AND org_id = ?",)
    .bind(body.name.trim())
    .bind(body.base_url.trim())
    .bind(&new_token)
    .bind(body.description.trim())
    .bind(&now)
    .bind(&id)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    Ok(Json(json!({
        "id": id,
        "name": body.name.trim(),
        "base_url": body.base_url.trim(),
        "bearer_token": mask_token(&new_token),
        "bearer_token_set": !new_token.is_empty(),
        "description": body.description.trim(),
        "updated_at": now,
    })))
}

// DELETE /v1/sandbox-envs/:id
pub(crate) async fn delete_sandbox_env(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller.sql_scope().to_string();

    let result = q!("DELETE FROM sandbox_env WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("sandbox environment not found".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

// GET /v1/spec-versions
pub(crate) async fn list_spec_versions(
    State(pool): State<sqlx::AnyPool>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    // Org isolation: mirror get_spec_version_raw — authenticated callers only see
    // spec versions for their own services. Empty org (desktop/no-auth) sees all.
    let org_id = caller.sql_scope().to_string();
    let rows = q!(
        r#"SELECT sv.id, sv.service_id, s.name AS service_name, sv.git_ref,
                  sv.spec_format, sv.captured_at
           FROM spec_version sv
           JOIN service s ON s.id = sv.service_id
           WHERE sv.spec_yaml IS NOT NULL
             AND (? = '' OR s.org_id = ?)
           ORDER BY sv.captured_at DESC
           LIMIT 100"#,
    )
    .bind(&org_id)
    .bind(&org_id)
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
pub(crate) async fn get_spec_version_raw(
    State(pool): State<sqlx::AnyPool>,
    Path(id): Path<String>,
    caller: CallerOrg,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let row = q!(
        "SELECT sv.spec_yaml, sv.spec_format, s.org_id AS service_org_id \
         FROM spec_version sv \
         JOIN service s ON s.id = sv.service_id \
         WHERE sv.id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("spec version not found".into()))?;

    let caller_org_id = caller.sql_scope().to_string();
    let row_org_id: String = row.try_get("service_org_id").unwrap_or_default();
    assert_org_access(&row_org_id, &caller_org_id, "spec version")?;

    let spec_yaml: Option<String> = row.try_get("spec_yaml").ok().flatten();
    let spec_format: String = row
        .try_get("spec_format")
        .unwrap_or_else(|_| "openapi".into());

    let content =
        spec_yaml.ok_or_else(|| ApiError::NotFound("no spec stored for this version".into()))?;

    let content_type = if spec_format.contains("json") || content.trim_start().starts_with('{') {
        "application/json"
    } else {
        "application/yaml"
    };

    Ok(([(axum::http::header::CONTENT_TYPE, content_type)], content))
}

#[cfg(test)]
mod tests {
    use super::mask_token;

    #[test]
    fn mask_token_reveals_last_4_ascii() {
        assert_eq!(mask_token("abcdefgh"), "***efgh");
    }

    #[test]
    fn mask_token_short_token_fully_masked() {
        assert_eq!(mask_token("abcd"), "***");
        assert_eq!(mask_token("ab"), "***");
    }

    #[test]
    fn mask_token_multibyte_utf8_does_not_panic() {
        // "€" is 3 bytes; a naive byte slice `&t[len-4..]` would land mid-char and panic.
        let masked = mask_token("€€€€€");
        assert_eq!(masked, "***€€€€");
    }
}
