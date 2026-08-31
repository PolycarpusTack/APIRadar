use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde_json::json;
use std::sync::OnceLock;

pub(crate) enum ApiError {
    Db(sqlx::Error),
    /// Renders as HTTP 422. The request parsed but failed semantic
    /// validation; named for the status it actually returns rather than for
    /// 400, which it does not.
    Unprocessable(String),
    NotFound(String),
    Forbidden(String),
    Unauthorized,
    TooManyRequests(String),
    UnprocessableEntity {
        error: String,
        detail: String,
        spec: String,
    },
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Db(e) => write!(f, "database error: {e}"),
            ApiError::Unprocessable(m)
            | ApiError::NotFound(m)
            | ApiError::Forbidden(m)
            | ApiError::TooManyRequests(m) => write!(f, "{m}"),
            ApiError::Unauthorized => write!(f, "unauthorized"),
            ApiError::UnprocessableEntity { error, .. } => write!(f, "{error}"),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Db(e)
    }
}

/// Map a database error from a batch ingest to an `ApiError`, translating
/// user-caused constraint violations (foreign-key / check) into a 4xx
/// `BadRequest` instead of a 500. A FK violation here means the client sent a
/// row referencing an unknown `consumer_id`/`service_id` — client error, not a
/// server fault. Other database errors remain 500s.
pub(crate) fn map_ingest_db_error(e: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(db) = &e {
        if db.is_foreign_key_violation() || db.is_check_violation() {
            return ApiError::Unprocessable(
                "batch references an unknown consumer_id or service_id".to_string(),
            );
        }
    }
    ApiError::Db(e)
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
            ApiError::Unprocessable(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": msg})),
            )
                .into_response(),
            ApiError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, Json(json!({"error": msg}))).into_response()
            }
            ApiError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, Json(json!({"error": msg}))).into_response()
            }
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response(),
            ApiError::TooManyRequests(msg) => {
                (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": msg}))).into_response()
            }
            ApiError::UnprocessableEntity {
                error,
                detail,
                spec,
            } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({"error": error, "detail": detail, "spec": spec})),
            )
                .into_response(),
        }
    }
}

static PROMETHEUS: OnceLock<PrometheusHandle> = OnceLock::new();

pub(crate) fn get_prometheus_handle() -> &'static PrometheusHandle {
    PROMETHEUS.get_or_init(|| {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder).ok();
        handle
    })
}
