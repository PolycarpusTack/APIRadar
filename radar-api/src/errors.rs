use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde_json::json;
use std::sync::OnceLock;

pub(crate) enum ApiError {
    Db(sqlx::Error),
    BadRequest(String),
    NotFound(String),
    Forbidden(String),
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
            ApiError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
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

static PROMETHEUS: OnceLock<PrometheusHandle> = OnceLock::new();

pub(crate) fn get_prometheus_handle() -> &'static PrometheusHandle {
    PROMETHEUS.get_or_init(|| {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder).ok();
        handle
    })
}
