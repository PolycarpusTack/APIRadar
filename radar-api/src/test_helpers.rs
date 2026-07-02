#![allow(dead_code)] // fixture library — methods are available for F+ tests even if not yet used
use axum::{body::Body, http::Request, http::StatusCode, Router};
use http_body_util::BodyExt;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// In-process echo/mock HTTP server
// ---------------------------------------------------------------------------

/// A lightweight HTTP server that binds to port 0 (OS-assigned) and records
/// every request it receives.  Used in integration tests that need a real
/// outbound HTTP target (webhook delivery, scheduled-scan execution) without
/// network access.
///
/// Drop the returned [`EchoServer`] to shut the server down.
pub(crate) struct EchoServer {
    pub addr: std::net::SocketAddr,
    pub requests: std::sync::Arc<tokio::sync::Mutex<Vec<EchoRequest>>>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

#[derive(Debug, Clone)]
pub(crate) struct EchoRequest {
    pub method: String,
    pub path: String,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl EchoServer {
    /// Wait until the server has recorded at least `n` requests (with timeout).
    pub(crate) async fn wait_for_requests(&self, n: usize, timeout_ms: u64) {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);
        loop {
            if self.requests.lock().await.len() >= n {
                return;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting for {n} requests to echo server");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        }
    }
}

/// Spawn an in-process echo server and return its address.
/// The server responds 200 OK to every request.
/// Configured status can be set per-call via the query param `?status=<code>`.
pub(crate) async fn spawn_echo_server() -> EchoServer {
    use axum::{extract::Request as AxumRequest, response::IntoResponse};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let requests: Arc<Mutex<Vec<EchoRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let req_clone = Arc::clone(&requests);

    let app = axum::Router::new().fallback(axum::routing::any(move |req: AxumRequest| {
        let store = Arc::clone(&req_clone);
        async move {
            let method = req.method().to_string();
            let path = req
                .uri()
                .path_and_query()
                .map(|p| p.as_str().to_owned())
                .unwrap_or_default();
            let headers: Vec<(String, String)> = req
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_owned()))
                .collect();
            let body_bytes = axum::body::to_bytes(req.into_body(), 1_048_576)
                .await
                .unwrap_or_default();
            let body = String::from_utf8_lossy(&body_bytes).to_string();
            // Parse optional ?status= query param for configurable responses.
            let status_code = path
                .split_once('?')
                .and_then(|(_, q)| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .find(|(k, _)| k == "status")
                        .and_then(|(_, v)| v.parse::<u16>().ok())
                })
                .unwrap_or(200);
            store.lock().await.push(EchoRequest {
                method,
                path,
                body,
                headers,
            });
            (
                axum::http::StatusCode::from_u16(status_code).unwrap_or(axum::http::StatusCode::OK),
                "ok",
            )
                .into_response()
        }
    }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .ok();
    });

    EchoServer {
        addr,
        requests,
        _shutdown: tx,
    }
}

pub(crate) struct TestClient {
    app: Router,
}

impl TestClient {
    pub(crate) fn new(pool: sqlx::AnyPool) -> Self {
        TestClient {
            app: super::build_router(pool, None, 4 * 1024 * 1024, false, None),
        }
    }

    /// Build a client that injects a `JwtClaims` extension with the given
    /// `org_id` on every request.  Use this for tests that need data scoped
    /// to a specific org without going through real JWT validation.
    pub(crate) fn new_with_jwt(pool: sqlx::AnyPool, org_id: &str) -> Self {
        let claims = super::JwtClaims {
            sub: "test-user".into(),
            org_id: org_id.to_string(),
            exp: usize::MAX,
        };
        let app = super::build_router(pool, None, 4 * 1024 * 1024, false, None).layer(
            axum::middleware::from_fn(
                move |mut req: axum::http::Request<axum::body::Body>,
                      next: axum::middleware::Next| {
                    let c = claims.clone();
                    async move {
                        req.extensions_mut().insert(c);
                        next.run(req).await
                    }
                },
            ),
        );
        TestClient { app }
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
        let bytes = resp
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
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
