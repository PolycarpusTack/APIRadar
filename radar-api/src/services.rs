use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::{JwtClaims, assert_org_access};
use crate::errors::ApiError;

#[derive(serde::Deserialize)]
pub(crate) struct CreateServiceBody {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) repo_url: String,
    pub(crate) owner_team: String,
    pub(crate) spec_format: String,
}

// POST /v1/services — explicitly register a Producer service
pub(crate) async fn create_service(
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
pub(crate) async fn get_service(
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
            assert_org_access(&row_org_id, &caller_org_id, &format!("service {service_id}"))?;
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
pub(crate) async fn list_services(
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
