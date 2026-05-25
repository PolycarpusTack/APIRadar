#![allow(dead_code)] // fixture library — methods are available for F+ tests even if not yet used
use axum::{body::Body, http::Request, http::StatusCode, Router};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

pub(crate) struct TestClient {
    app: Router,
}

impl TestClient {
    pub(crate) fn new(pool: sqlx::AnyPool) -> Self {
        TestClient {
            app: super::build_router(pool, None, 4 * 1024 * 1024, false, None),
        }
    }

    pub(crate) async fn get(&self, uri: &str) -> TestResponse {
        self.send(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    pub(crate) async fn post_json(&self, uri: &str, body: &serde_json::Value) -> TestResponse {
        self.send(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    pub(crate) async fn patch_json(&self, uri: &str, body: &serde_json::Value) -> TestResponse {
        self.send(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    pub(crate) async fn delete(&self, uri: &str) -> TestResponse {
        self.send(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    async fn send(&self, req: Request<Body>) -> TestResponse {
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        TestResponse { status, bytes }
    }
}

pub(crate) struct TestResponse {
    status: StatusCode,
    bytes: Vec<u8>,
}

impl TestResponse {
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.bytes).expect("response was not valid JSON")
    }

    pub(crate) fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes).expect("response was not valid UTF-8")
    }
}
