//! O-5: `radar scan` must post `--collection` Evidence even when the code scan
//! finds zero call sites (a collections-only Consumer repo).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::{routing::post, Json, Router};

#[derive(Clone, Default)]
struct Hits {
    call_sites: Arc<AtomicUsize>,
    upserts: Arc<AtomicUsize>,
    collection_evidence: Arc<AtomicUsize>,
}

/// Start a minimal mock radar-api on an ephemeral port; returns its base URL.
async fn start_mock_api(hits: Hits) -> String {
    let cs = hits.call_sites.clone();
    let up = hits.upserts.clone();
    let ce = hits.collection_evidence.clone();

    let app = Router::new()
        .route(
            "/v1/call-sites",
            post(move |Json(body): Json<serde_json::Value>| {
                let cs = cs.clone();
                async move {
                    let n = body.as_array().map(|a| a.len()).unwrap_or(0);
                    cs.fetch_add(n, Ordering::SeqCst);
                    Json(serde_json::json!({ "accepted": n }))
                }
            }),
        )
        .route(
            "/v1/consumers/upsert",
            post(move |Json(_body): Json<serde_json::Value>| {
                let up = up.clone();
                async move {
                    up.fetch_add(1, Ordering::SeqCst);
                    Json(serde_json::json!({ "id": "con_mock", "created": true }))
                }
            }),
        )
        .route(
            "/v1/evidence/collection",
            post(move |Json(body): Json<serde_json::Value>| {
                let ce = ce.clone();
                async move {
                    let n = body.as_array().map(|a| a.len()).unwrap_or(0);
                    ce.fetch_add(n, Ordering::SeqCst);
                    Json(serde_json::json!({ "accepted": n, "inserted": n }))
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock api");
    let addr = listener.local_addr().expect("mock api addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock api serve");
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn collection_only_scan_posts_evidence() {
    let hits = Hits::default();
    let api_url = start_mock_api(hits.clone()).await;

    // An empty source dir: the code scan finds nothing.
    let empty_src = tempfile::tempdir().expect("tempdir");

    // A real collection fixture with requests + test-script field assertions.
    let collection = std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/billing-svc-tests.postman_collection.json"
    ));
    assert!(collection.exists(), "fixture missing: {collection:?}");

    radar_cli_lib::scan::run_scan(
        &api_url,
        "con_fallback",
        "svc_billing",
        empty_src.path(),
        None,
        &HashMap::new(),
        std::slice::from_ref(&collection),
        false,
    )
    .await
    .expect("run_scan failed");

    assert!(
        hits.upserts.load(Ordering::SeqCst) > 0,
        "the collection's consumer must be upserted even with zero code call sites"
    );
    assert!(
        hits.collection_evidence.load(Ordering::SeqCst) > 0,
        "collection Evidence must be posted even when the code scan finds nothing"
    );
}

#[tokio::test]
async fn scan_with_no_records_and_no_collections_is_ok() {
    let hits = Hits::default();
    let api_url = start_mock_api(hits.clone()).await;
    let empty_src = tempfile::tempdir().expect("tempdir");

    radar_cli_lib::scan::run_scan(
        &api_url,
        "con_x",
        "svc_x",
        empty_src.path(),
        None,
        &HashMap::new(),
        &[],
        false,
    )
    .await
    .expect("run_scan failed");

    assert_eq!(hits.call_sites.load(Ordering::SeqCst), 0);
    assert_eq!(hits.collection_evidence.load(Ordering::SeqCst), 0);
}
