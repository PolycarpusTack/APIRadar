use anyhow::Result;
use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::any::AnyPoolOptions;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

enum ApiError {
    Db(sqlx::Error),
    BadRequest(String),
    Unauthorized,
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
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response(),
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn run(db_url: &str) -> Result<()> {
    sqlx::any::install_default_drivers();

    let pool = AnyPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    info!("migrations applied");

    let app = build_router(pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

async fn auth_middleware(
    State(pool): State<sqlx::AnyPool>,
    req: Request,
    next: Next,
) -> Response {
    // Only protect /v1/* routes — /health is always accessible.
    let path = req.uri().path().to_owned();
    if !path.starts_with("/v1/") {
        return next.run(req).await;
    }

    let token = std::env::var("DRIFT_SERVICE_TOKEN").unwrap_or_default();
    if token.is_empty() {
        // No token configured → auth disabled.
        return next.run(req).await;
    }

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected = format!("Bearer {token}");
    if auth_header != expected {
        drop(pool); // keep borrow happy
        return ApiError::Unauthorized.into_response();
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

fn build_router(pool: sqlx::AnyPool) -> Router {
    let v1 = Router::new()
        .route("/services/:id/diffs", get(list_diffs).post(create_diff))
        .route("/services/:id/consumers", get(list_consumers))
        .route("/consumers", post(create_consumer))
        .route("/diffs/:id/blast-radius", get(blast_radius))
        .route("/usage/events", post(ingest_usage_event))
        .layer(middleware::from_fn_with_state(pool.clone(), auth_middleware))
        .with_state(pool.clone());

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
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

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "version": "0.1.0"}))
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

    // 2. Upsert from_version spec_version row.
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

    // 3. Upsert to_version spec_version row.
    let to_version_id = spec_version_id(&service_id, &body.to_git_ref);
    sqlx::query(
        r#"
        INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(&to_version_id)
    .bind(&service_id)
    .bind(&body.to_git_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .execute(&pool)
    .await?;

    // 4. Insert diff row.
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

    // 5. Insert change rows.
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
async fn list_consumers(Path(_service_id): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

// POST /v1/consumers
async fn create_consumer() -> impl IntoResponse {
    StatusCode::CREATED
}

// GET /v1/diffs/:id/blast-radius
async fn blast_radius(Path(_diff_id): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

// POST /v1/usage/events
async fn ingest_usage_event() -> impl IntoResponse {
    StatusCode::ACCEPTED
}
