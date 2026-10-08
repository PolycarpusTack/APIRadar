//! Regression coverage for the October 2026 code review.
use crate::test_helpers::TestClient;
use axum::http::StatusCode;
use serde_json::{json, Value};

async fn test_pool() -> sqlx::AnyPool {
    sqlx::any::install_default_drivers();
    let url = crate::test_helpers::test_db_url();
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    crate::test_helpers::isolate_postgres_schema(&pool, &url).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn evidence_fixture() -> (sqlx::AnyPool, TestClient) {
    let pool = test_pool().await;
    q!("INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) VALUES ('sa', 'Producer A', '', '', 'openapi', 'a'), ('sb', 'Producer B', '', '', 'openapi', 'b')")
        .execute(&pool).await.unwrap();
    q!("INSERT INTO consumer (id, name, repo_url, owner_team, contact, org_id) VALUES ('ca', 'Consumer A', '', '', '', 'a'), ('cb', 'Consumer B', '', '', '', 'b')")
        .execute(&pool).await.unwrap();
    let client = TestClient::new_with_jwt(pool.clone(), "a");
    (pool, client)
}

fn usage_item(consumer: &str, service: &str) -> Value {
    json!({"consumer_id": consumer, "service_id": service, "operation": "GET /items"})
}

fn call_site_item(consumer: &str, service: &str) -> Value {
    json!({"consumer_id": consumer, "service_id": service, "operation": "GET /items", "file_path": "src/items.ts", "line_number": 1, "field_path": ""})
}

fn collection_item(consumer: &str, service: &str) -> Value {
    json!({"consumer_id": consumer, "service_id": service, "operation": "GET /items", "evidence_uri": "collection_file://items.json"})
}

fn gateway_item(consumer: &str, service: &str) -> Value {
    json!({"consumer_id": consumer, "service_id": service, "method": "GET", "path": "/items"})
}

fn otlp_item(consumer: &str, service: &str) -> Value {
    json!({"kind": 3, "attributes": [
        {"key": "radar.consumer_id", "value": {"stringValue": consumer}},
        {"key": "radar.service_id", "value": {"stringValue": service}},
        {"key": "http.method", "value": {"stringValue": "GET"}},
        {"key": "http.route", "value": {"stringValue": "/items"}}
    ]})
}

fn array_body(items: Vec<Value>) -> Value {
    json!(items)
}

fn otlp_body(items: Vec<Value>) -> Value {
    json!({"resourceSpans": [{"scopeSpans": [{"spans": items}]}]})
}

async fn assert_no_evidence(pool: &sqlx::AnyPool) {
    let count: i64 = qs!("SELECT (SELECT COUNT(*) FROM usage_event) + (SELECT COUNT(*) FROM call_site) + (SELECT COUNT(*) FROM impact_evidence)")
        .fetch_one(pool).await.unwrap();
    assert_eq!(
        count, 0,
        "rejected batches must not write even their valid first item"
    );
}

macro_rules! ingestion_regressions {
    ($module:ident, $endpoint:literal, $item:ident, $body:ident) => {
        mod $module {
            use super::*;

            #[tokio::test]
            async fn foreign_producer_rejects_whole_batch() {
                let (pool, client) = evidence_fixture().await;
                let body = $body(vec![$item("ca", "sa"), $item("ca", "sb")]);
                let response = client.post_json($endpoint, &body).await;
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{}",
                    response.text()
                );
                assert_no_evidence(&pool).await;
            }

            #[tokio::test]
            async fn foreign_consumer_rejects_whole_batch() {
                let (pool, client) = evidence_fixture().await;
                let body = $body(vec![$item("ca", "sa"), $item("cb", "sa")]);
                let response = client.post_json($endpoint, &body).await;
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{}",
                    response.text()
                );
                assert_no_evidence(&pool).await;
            }

            #[tokio::test]
            async fn owned_ids_are_accepted() {
                let (_pool, client) = evidence_fixture().await;
                let response = client
                    .post_json($endpoint, &$body(vec![$item("ca", "sa")]))
                    .await;
                assert_eq!(
                    response.status(),
                    StatusCode::ACCEPTED,
                    "{}",
                    response.text()
                );
                assert_eq!(response.json()["accepted"], 1);
            }
        }
    };
}

ingestion_regressions!(usage, "/v1/usage/events", usage_item, array_body);
ingestion_regressions!(call_sites, "/v1/call-sites", call_site_item, array_body);
ingestion_regressions!(
    collection,
    "/v1/evidence/collection",
    collection_item,
    array_body
);
ingestion_regressions!(gateway, "/v1/gateway/logs", gateway_item, array_body);
ingestion_regressions!(otlp, "/v1/otlp/v1/traces", otlp_item, otlp_body);

#[tokio::test]
async fn coverage_does_not_expose_foreign_names_from_legacy_poisoned_rows() {
    let (pool, client) = evidence_fixture().await;
    q!("INSERT INTO impact_evidence (id, org_id, diff_id, producer_service_id, consumer_id, source_type, confidence, observed_at) VALUES ('bad', 'a', '', 'sb', 'cb', 'collection_file', 'medium', '2026-01-01T00:00:00+00:00')")
        .execute(&pool).await.unwrap();

    let response = client.get("/v1/evidence/coverage").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.json(), json!([]));
}

const GQL_BASE: &str = "type Query { item: String other: String }";
const GQL_HEAD: &str = "type Query { other: String }";

fn comparison_body(base: &str, head: &str) -> Value {
    json!({"base_spec": base, "head_spec": head, "spec_format": "graphql", "base_ref": "before", "head_ref": "after"})
}

#[tokio::test]
async fn comparison_changed_head_creates_new_diff_and_preserves_history() {
    let (pool, client) = evidence_fixture().await;
    let first = client
        .post_json(
            "/v1/services/sa/diffs/compare",
            &comparison_body(GQL_BASE, GQL_BASE),
        )
        .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_id = first.json()["diff_id"].as_str().unwrap().to_string();

    let second = client
        .post_json(
            "/v1/services/sa/diffs/compare",
            &comparison_body(GQL_BASE, GQL_HEAD),
        )
        .await;

    assert_eq!(second.status(), StatusCode::CREATED);
    assert_ne!(second.json()["diff_id"], first_id);
    assert_eq!(second.json()["breaking_count"], 1);
    let original = client.get(&format!("/v1/diffs/{first_id}")).await;
    assert_eq!(original.json()["changes"], json!([]));
    let original_head: String = qs!("SELECT sv.spec_yaml FROM spec_version sv JOIN diff d ON d.to_version = sv.id WHERE d.id = ?")
        .bind(&first_id).fetch_one(&pool).await.unwrap();
    assert_eq!(original_head, GQL_BASE);
    let stored = client
        .get(&format!(
            "/v1/diffs/{}",
            second.json()["diff_id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(stored.json()["changes"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn comparison_changed_base_creates_new_diff_and_preserves_base() {
    let (pool, client) = evidence_fixture().await;
    let first = client
        .post_json(
            "/v1/services/sa/diffs/compare",
            &comparison_body(GQL_BASE, GQL_HEAD),
        )
        .await;
    let first_id = first.json()["diff_id"].as_str().unwrap().to_string();

    let second = client
        .post_json(
            "/v1/services/sa/diffs/compare",
            &comparison_body(GQL_HEAD, GQL_HEAD),
        )
        .await;

    assert_eq!(second.status(), StatusCode::CREATED);
    assert_ne!(second.json()["diff_id"], first_id);
    assert_eq!(second.json()["changes_count"], 0);
    let original_base: String = qs!("SELECT sv.spec_yaml FROM spec_version sv JOIN diff d ON d.from_version = sv.id WHERE d.id = ?")
        .bind(&first_id).fetch_one(&pool).await.unwrap();
    assert_eq!(original_base, GQL_BASE);
}

#[tokio::test]
async fn comparison_identical_content_and_refs_are_idempotent() {
    let (_pool, client) = evidence_fixture().await;
    let body = comparison_body(GQL_BASE, GQL_HEAD);
    let first = client
        .post_json("/v1/services/sa/diffs/compare", &body)
        .await;

    let second = client
        .post_json("/v1/services/sa/diffs/compare", &body)
        .await;

    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(second.json()["diff_id"], first.json()["diff_id"]);
    assert_eq!(second.json()["cached"], true);
    assert_eq!(second.json()["breaking_count"], 1);
}

fn submitted_diff_body(head: &str, changes: Value) -> Value {
    json!({"service_name": "Producer A", "repo_url": "", "owner_team": "", "from_git_ref": "before", "to_git_ref": "after", "spec_format": "graphql", "spec_yaml": head, "changes": changes})
}

#[tokio::test]
async fn submitted_diff_changed_content_does_not_overwrite_snapshot() {
    let (pool, client) = evidence_fixture().await;
    let first = client
        .post_json(
            "/v1/services/sa/diffs",
            &submitted_diff_body(GQL_BASE, json!([])),
        )
        .await;
    let first_id = first.json()["id"].as_str().unwrap().to_string();
    let change = json!([{"path": "Query.item", "kind": "field_removed", "severity": "breaking"}]);

    let second = client
        .post_json(
            "/v1/services/sa/diffs",
            &submitted_diff_body(GQL_HEAD, change),
        )
        .await;

    assert_eq!(second.status(), StatusCode::CREATED);
    assert_ne!(second.json()["id"], first_id);
    let original: String = qs!("SELECT sv.spec_yaml FROM spec_version sv JOIN diff d ON d.to_version = sv.id WHERE d.id = ?")
        .bind(first_id).fetch_one(&pool).await.unwrap();
    assert_eq!(original, GQL_BASE);
}

#[tokio::test]
async fn submitted_diff_changed_report_with_same_head_is_not_stale_cached() {
    let (_pool, client) = evidence_fixture().await;
    let first = client
        .post_json(
            "/v1/services/sa/diffs",
            &submitted_diff_body(GQL_HEAD, json!([])),
        )
        .await;
    let change = json!([{"path": "Query.item", "kind": "field_removed", "severity": "breaking"}]);

    let second = client
        .post_json(
            "/v1/services/sa/diffs",
            &submitted_diff_body(GQL_HEAD, change),
        )
        .await;

    assert_eq!(second.status(), StatusCode::CREATED);
    assert_ne!(second.json()["id"], first.json()["id"]);
}

#[tokio::test]
async fn submitted_diff_reordered_report_remains_idempotent() {
    let (_pool, client) = evidence_fixture().await;
    let removed = json!({"path": "Query.item", "kind": "field_removed", "severity": "breaking"});
    let added = json!({"path": "Query.newItem", "kind": "field_added", "severity": "safe"});
    let first = client
        .post_json(
            "/v1/services/sa/diffs",
            &submitted_diff_body(GQL_HEAD, json!([removed, added])),
        )
        .await;

    let second = client
        .post_json(
            "/v1/services/sa/diffs",
            &submitted_diff_body(GQL_HEAD, json!([added, removed])),
        )
        .await;

    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(second.json()["id"], first.json()["id"]);
}

async fn collection_blast_fixture(path: &str) -> (sqlx::AnyPool, TestClient, String) {
    let (pool, client) = evidence_fixture().await;
    let body = submitted_diff_body(
        GQL_HEAD,
        json!([{"path": path, "kind": "field_removed", "severity": "breaking"}]),
    );
    let response = client.post_json("/v1/services/sa/diffs", &body).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let id = response.json()["id"].as_str().unwrap().to_string();
    (pool, client, id)
}

#[tokio::test]
async fn collection_evidence_matches_operation_without_subscription() {
    let (pool, client, diff_id) = collection_blast_fixture("GET /items").await;
    let accepted = client
        .post_json(
            "/v1/evidence/collection",
            &json!([collection_item("ca", "sa")]),
        )
        .await;
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    let entries = response.json()["entries"].as_array().unwrap().clone();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["consumer"]["id"], "ca");
    assert_eq!(entries[0]["confidence"], "medium");
    assert_eq!(entries[0]["evidence"][0]["kind"], "collection_file");
    assert_eq!(
        entries[0]["evidence"][0]["evidence_uri"],
        "collection_file://items.json"
    );
    assert!(!entries[0]["last_seen"].as_str().unwrap().is_empty());
    let repeated = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;
    assert_eq!(repeated.json()["entries"], json!(entries));
    let count: i64 = qs!("SELECT COUNT(*) FROM impact_evidence")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "collection Evidence is already durable; reading it must not clone it"
    );
}

#[tokio::test]
async fn collection_evidence_matches_changed_field_and_operation() {
    let (_pool, client, diff_id) = collection_blast_fixture("GET /items → response.name").await;
    client.post_json("/v1/evidence/collection", &json!([{"consumer_id": "ca", "service_id": "sa", "operation": "GET /items", "field_path": "name"}])).await;

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    assert_eq!(response.json()["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn collection_evidence_with_unknown_fields_conservatively_matches_operation() {
    let (_pool, client, diff_id) = collection_blast_fixture("GET /items → response.name").await;
    client
        .post_json(
            "/v1/evidence/collection",
            &json!([collection_item("ca", "sa")]),
        )
        .await;

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    assert_eq!(response.json()["entries"].as_array().unwrap().len(), 1);
    assert_eq!(response.json()["entries"][0]["confidence"], "medium");
}

#[tokio::test]
async fn collection_evidence_different_operation_does_not_match_field() {
    let (_pool, client, diff_id) = collection_blast_fixture("GET /items → response.name").await;
    client.post_json("/v1/evidence/collection", &json!([{"consumer_id": "ca", "service_id": "sa", "operation": "GET /other", "field_path": "name"}])).await;

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    assert_eq!(response.json()["entries"], json!([]));
}

#[tokio::test]
async fn collection_evidence_different_field_does_not_match() {
    let (_pool, client, diff_id) = collection_blast_fixture("GET /items → response.name").await;
    client.post_json("/v1/evidence/collection", &json!([{"consumer_id": "ca", "service_id": "sa", "operation": "GET /items", "field_path": "unrelated"}])).await;

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    assert_eq!(response.json()["entries"], json!([]));
}

#[tokio::test]
async fn expired_collection_evidence_is_not_in_blast_radius() {
    let (pool, client, diff_id) = collection_blast_fixture("GET /items").await;
    client
        .post_json(
            "/v1/evidence/collection",
            &json!([collection_item("ca", "sa")]),
        )
        .await;
    q!("UPDATE impact_evidence SET expires_at = '2000-01-01T00:00:00+00:00'")
        .execute(&pool)
        .await
        .unwrap();

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    assert_eq!(response.json()["entries"], json!([]));
}

#[tokio::test]
async fn foreign_collection_evidence_is_not_in_blast_radius() {
    let (pool, client, diff_id) = collection_blast_fixture("GET /items").await;
    client
        .post_json(
            "/v1/evidence/collection",
            &json!([collection_item("ca", "sa")]),
        )
        .await;
    q!("UPDATE impact_evidence SET org_id = 'b'")
        .execute(&pool)
        .await
        .unwrap();

    let response = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius"))
        .await;

    assert_eq!(response.json()["entries"], json!([]));
}

#[tokio::test]
async fn collection_evidence_respects_max_age_days() {
    let (pool, client, diff_id) = collection_blast_fixture("GET /items").await;
    client
        .post_json(
            "/v1/evidence/collection",
            &json!([collection_item("ca", "sa")]),
        )
        .await;
    let old = (chrono::Utc::now() - chrono::Duration::days(10)).to_rfc3339();
    q!("UPDATE impact_evidence SET observed_at = ?")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();

    let recent_only = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius?max_age_days=7"))
        .await;
    let wider = client
        .get(&format!("/v1/diffs/{diff_id}/blast-radius?max_age_days=30"))
        .await;

    assert_eq!(recent_only.json()["entries"], json!([]));
    assert_eq!(wider.json()["entries"].as_array().unwrap().len(), 1);
}
