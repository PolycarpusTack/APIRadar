use anyhow::Result;
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::any::AnyPoolOptions;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Initialise the database, build the router, and serve on 0.0.0.0:8080.
pub async fn run(db_url: &str) -> Result<()> {
    // Install drivers for both SQLite and PostgreSQL so AnyPool works.
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
// Router
// ---------------------------------------------------------------------------

fn build_router(pool: sqlx::AnyPool) -> Router {
    // `pool` is kept in app state for future use by real handlers.
    Router::new()
        .route("/health", get(health))
        .route("/v1/services/:id/diffs", get(list_diffs).post(create_diff))
        .route("/v1/services/:id/consumers", get(list_consumers))
        .route("/v1/consumers", post(create_consumer))
        .route("/v1/diffs/:id/blast-radius", get(blast_radius))
        .route("/v1/usage/events", post(ingest_usage_event))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(pool)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "version": "0.1.0"}))
}

async fn list_diffs(Path(_service_id): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn create_diff(Path(_service_id): Path<String>) -> impl IntoResponse {
    StatusCode::CREATED
}

async fn list_consumers(Path(_service_id): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!([])))
}

async fn create_consumer() -> impl IntoResponse {
    StatusCode::CREATED
}

async fn blast_radius(Path(_diff_id): Path<String>) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({})))
}

async fn ingest_usage_event() -> impl IntoResponse {
    StatusCode::ACCEPTED
}
