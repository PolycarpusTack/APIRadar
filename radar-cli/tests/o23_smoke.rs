//! O-23 — EPIC O smoke test: the whole promise, end to end, built from the
//! exact shapes the 2026-09-01 review found broken.
//!
//! detect (diff engines, incl. O-1/O-2/O-3 blind spots) →
//! attribute (collection Evidence with O-8 normalization, via a mock API) →
//! decide (policy engine) → explain (O-14 truthful PR comment).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::{routing::post, Json, Router};
use radar_cli_lib::{github, policy};
use radar_core::models::{ChangeKind, Severity};

const BASE_YAML: &str = include_str!("../../fixtures/o23-smoke/base.yaml");
const HEAD_YAML: &str = include_str!("../../fixtures/o23-smoke/head.yaml");
const BASE_SDL: &str = include_str!("../../fixtures/o23-smoke/schema-base.graphql");
const HEAD_SDL: &str = include_str!("../../fixtures/o23-smoke/schema-head.graphql");

/// Detect: the paginated-list rewrite (O-1) and the request oneOf required
/// flip (O-2) are both Breaking — and nothing else is falsely reported.
#[test]
fn smoke_detects_both_breaking_changes_and_nothing_else() {
    let base = radar_core::diff::parse_openapi(BASE_YAML).expect("base parses");
    let head = radar_core::diff::parse_openapi(HEAD_YAML).expect("head parses");
    let changes = radar_core::diff::diff_openapi(&base, &head);

    assert!(
        changes.iter().any(|c| c.kind == ChangeKind::TypeChanged
            && c.severity == Severity::Breaking
            && c.path == "GET /users \u{2192} response"),
        "array → paginated object must be Breaking at the response root; got {changes:?}"
    );
    assert!(
        changes.iter().any(|c| c.kind == ChangeKind::RequiredChanged
            && c.severity == Severity::Breaking
            && c.path.contains("request_body")),
        "optional → required inside the request oneOf must be Breaking; got {changes:?}"
    );
    // Zero false positives: every reported change is one of the two above.
    for c in &changes {
        assert!(
            c.path.starts_with("GET /users") || c.path.starts_with("POST /orders"),
            "unexpected change reported: {c:?}"
        );
    }
    // And both specs self-diff clean.
    assert!(radar_core::diff::diff_openapi(&base, &base).is_empty());
    assert!(radar_core::diff::diff_openapi(&head, &head).is_empty());
}

/// Detect (GraphQL): a schema with custom scalars self-diffs to zero (O-3)
/// and still reports the real Breaking Change.
#[test]
fn smoke_graphql_scalars_quiet_real_break_loud() {
    let base = radar_core::graphql::parse_graphql(BASE_SDL).expect("base sdl");
    let head = radar_core::graphql::parse_graphql(HEAD_SDL).expect("head sdl");

    assert!(
        radar_core::graphql::diff_graphql(&base, &base).is_empty(),
        "scalar-bearing SDL must self-diff clean"
    );
    let changes = radar_core::graphql::diff_graphql(&base, &head);
    assert!(
        changes
            .iter()
            .any(|c| c.severity == Severity::Breaking && c.path.contains("phone")),
        "the removed User.phone field must be Breaking; got {changes:?}"
    );
    assert!(
        !changes.iter().any(|c| c.path.contains("DateTime")),
        "the unchanged scalar must not appear; got {changes:?}"
    );
}

/// Attribute: a collections-only Consumer posts Evidence (O-5) whose
/// operation is normalized to match the spec (O-8).
#[tokio::test]
async fn smoke_collection_only_consumer_produces_matching_evidence() {
    let evidence_ops: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let upserts = Arc::new(AtomicUsize::new(0));

    let ops = evidence_ops.clone();
    let ups = upserts.clone();
    let app = Router::new()
        .route(
            "/v1/consumers/upsert",
            post(move |Json(_): Json<serde_json::Value>| {
                let ups = ups.clone();
                async move {
                    ups.fetch_add(1, Ordering::SeqCst);
                    Json(serde_json::json!({ "id": "con_smoke", "created": true }))
                }
            }),
        )
        .route(
            "/v1/evidence/collection",
            post(move |Json(body): Json<serde_json::Value>| {
                let ops = ops.clone();
                async move {
                    let n = body.as_array().map(|a| a.len()).unwrap_or(0);
                    for item in body.as_array().into_iter().flatten() {
                        if let Some(op) = item["operation"].as_str() {
                            ops.lock().unwrap().push(op.to_string());
                        }
                    }
                    Json(serde_json::json!({ "accepted": n, "inserted": n }))
                }
            }),
        )
        .route(
            "/v1/call-sites",
            post(|Json(body): Json<serde_json::Value>| async move {
                let n = body.as_array().map(|a| a.len()).unwrap_or(0);
                Json(serde_json::json!({ "accepted": n }))
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    let empty_src = tempfile::tempdir().unwrap();
    let collection = std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/o23-smoke/mobile.postman_collection.json"
    ));
    radar_cli_lib::scan::run_scan(
        &format!("http://{addr}"),
        "con_fallback",
        "svc_smoke",
        empty_src.path(),
        None,
        &HashMap::new(),
        std::slice::from_ref(&collection),
    )
    .await
    .expect("run_scan");

    assert!(upserts.load(Ordering::SeqCst) > 0, "consumer must register");
    let ops = evidence_ops.lock().unwrap();
    assert!(
        ops.iter().any(|o| o == "/users/{userId}"),
        "the {{{{base_url}}}}/users/{{{{userId}}}} request must normalize to a \
         spec-matchable operation; got {ops:?}"
    );
}

/// Decide + explain: the verdict follows policy, and the PR comment tells the
/// truth about a coverage block, naming the configured override label (O-14).
#[test]
fn smoke_policy_decides_and_comment_tells_the_truth() {
    let base = radar_core::diff::parse_openapi(BASE_YAML).expect("base parses");
    let head = radar_core::diff::parse_openapi(HEAD_YAML).expect("head parses");
    let changes = radar_core::diff::diff_openapi(&base, &head);

    let pol = policy::PolicyConfig {
        block_on: policy::BlockOn::ActiveConsumers,
        allow_override_with: Some("api-break-approved".into()),
        ..Default::default()
    };
    let decision = policy::decide(
        &changes,
        &pol,
        &policy::FailMode::Closed,
        policy::ConsumerEvidence::Unknown,
        false,
        false,
    );
    assert_eq!(
        decision.verdict,
        policy::Verdict::InsufficientCoverage,
        "Breaking + Unknown coverage must block for want of Evidence (FIT-01)"
    );
    assert_eq!(decision.exit_code, 1);

    let comment = github::build_comment_with_suites(
        &changes,
        "v1",
        "v2",
        None,
        decision.verdict.wire_str(),
        "closed",
        &[],
        true,
        "api-break-approved",
    );
    assert!(
        comment.contains("no Evidence"),
        "the comment must state the real reason; got:\n{comment}"
    );
    assert!(
        !comment.contains("At least 1 high-confidence"),
        "the old false claim must be gone; got:\n{comment}"
    );
    assert!(
        comment.contains("api-break-approved"),
        "the configured override label must be named; got:\n{comment}"
    );
}
