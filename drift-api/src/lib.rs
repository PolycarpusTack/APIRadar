use anyhow::Result;
use axum::{
    extract::{Path, Request, State},
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
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use uuid::Uuid;

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

pub fn build_router(pool: sqlx::AnyPool) -> Router {
    let v1 = Router::new()
        .route("/services", get(list_services))
        .route("/services/:id/diffs", get(list_diffs).post(create_diff))
        .route("/services/:id/consumers", get(list_consumers))
        .route("/services/:id/subscriptions", post(create_subscription))
        .route("/consumers", get(list_all_consumers).post(create_consumer))
        .route("/diffs", get(list_all_diffs))
        .route("/diffs/:id", get(get_diff))
        .route("/diffs/:id/blast-radius", get(blast_radius))
        .route("/usage/events", post(ingest_usage_event))
        .route("/summary", get(get_summary))
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
    Json(body): Json<CreateConsumerBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if body.repo_url.trim().is_empty() {
        return Err(ApiError::BadRequest("repo_url must not be empty".into()));
    }

    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO consumer (id, name, repo_url, owner_team, contact)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(&id)
    .bind(&body.name)
    .bind(&body.repo_url)
    .bind(&body.owner_team)
    .bind(&body.contact)
    .execute(&pool)
    .await?;

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
               d.pr_url, d.created_at
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
        ORDER BY rowid
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

    // Parse each change path into (operation, field_path).
    // Path format: "GET /users" or "GET /users → response.phone"
    let mut changed_ops: Vec<String> = Vec::new();
    let mut changed_fields: Vec<(String, String)> = Vec::new(); // (operation, field_path)

    for row in &change_rows {
        let path: String = row.try_get("path").map_err(ApiError::Db)?;
        if let Some(arrow_pos) = path.find(" \u{2192} ") {
            let op = path[..arrow_pos].to_string();
            let after_arrow = &path[arrow_pos + " → ".len()..];
            // Strip "response." prefix if present
            let field = if let Some(stripped) = after_arrow.strip_prefix("response.") {
                stripped.to_string()
            } else {
                after_arrow.to_string()
            };
            changed_fields.push((op.clone(), field));
            if !changed_ops.contains(&op) {
                changed_ops.push(op);
            }
        } else {
            // Plain operation path e.g. "GET /users"
            if !changed_ops.contains(&path) {
                changed_ops.push(path.clone());
            }
        }
    }

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

        // Query usage_event for matching operations/fields within last 30 days.
        // Build dynamic OR conditions since AnyPool doesn't support array binding.
        let mut usage_last_seen: Option<String> = None;

        if !changed_ops.is_empty() {
            // Build query: match by operation for operation-level changes,
            // and by (operation, field_path) for field-level changes.
            // We query for any usage_event where operation matches one of the changed ops
            // OR (operation matches AND field_path matches for field-level changes).
            // Simplified: match consumer+service+recorded_at, then filter operation IN changed_ops.
            let mut sql = String::from(
                "SELECT recorded_at FROM usage_event WHERE consumer_id = ? AND service_id = ? AND recorded_at >= ? AND (",
            );
            for (i, _op) in changed_ops.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("operation = ?");
            }
            sql.push_str(") ORDER BY recorded_at DESC LIMIT 1");

            let mut q = sqlx::query(&sql)
                .bind(&consumer_id)
                .bind(&service_id)
                .bind(&cutoff_30);
            for op in &changed_ops {
                q = q.bind(op);
            }

            if let Some(row) = q.fetch_optional(&pool).await? {
                let ts: String = row.try_get("recorded_at").map_err(ApiError::Db)?;
                usage_last_seen = Some(ts);
            }
        }

        // Query call_site for static references matching changed operations.
        let mut call_site_last_seen: Option<String> = None;

        if !changed_ops.is_empty() {
            let mut sql = String::from(
                "SELECT last_seen_at FROM call_site WHERE consumer_id = ? AND service_id = ? AND (",
            );
            for (i, _op) in changed_ops.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" OR ");
                }
                sql.push_str("operation = ?");
            }
            sql.push_str(") ORDER BY last_seen_at DESC LIMIT 1");

            let mut q = sqlx::query(&sql).bind(&consumer_id).bind(&service_id);
            for op in &changed_ops {
                q = q.bind(op);
            }

            if let Some(row) = q.fetch_optional(&pool).await? {
                let ts: String = row.try_get("last_seen_at").map_err(ApiError::Db)?;
                call_site_last_seen = Some(ts);
            }
        }

        // Skip consumers with no evidence of using the changed paths.
        if usage_last_seen.is_none() && call_site_last_seen.is_none() {
            continue;
        }

        let has_runtime_usage = usage_last_seen.is_some();
        let has_call_site = call_site_last_seen.is_some();

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

// GET /v1/diffs — all diffs across all services (last 100, newest first)
async fn list_all_diffs(
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

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
        LIMIT 100
        "#,
    )
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

// GET /v1/services — list all registered Producer services
async fn list_services(
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let rows = sqlx::query(
        "SELECT id, name, repo_url, owner_team, spec_format FROM service ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;

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
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let rows = sqlx::query(
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
    .await?;

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
// Retention job (public, callable on a schedule)
// ---------------------------------------------------------------------------

/// Delete usage_event rows older than `lookback_days` days.
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

        let app = build_router(pool);

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
        let app = build_router(pool);

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
        let app = build_router(pool);

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

        let app = build_router(pool);

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

        let app = build_router(pool);

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

        let app = build_router(pool);
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
        let app = build_router(pool);

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
        let app = build_router(pool);

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

        let app = build_router(pool);

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

        let app = build_router(pool);

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

        let app = build_router(pool);

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
}
