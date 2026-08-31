/// SLO benchmarks for radar-api.
///
/// EPIC B targets:
///   blast-radius query  p95 < 300 ms
///   usage ingest batch  p99 < 100 ms  (batch of 100 events)
///
/// Run with:
///   cargo bench -p radar-api
///
/// View HTML reports:
///   target/criterion/report/index.html
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use radar_api::bench_support::{bench_pool, seed_blast_radius};
use radar_api::build_router;
use std::time::Duration;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Seed helpers live in radar_api::bench_support so tests/slo_guard.rs can
// measure exactly this scenario.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Bench 1 — Blast-radius query  (SLO: p95 < 300 ms)
// ---------------------------------------------------------------------------

fn bench_blast_radius(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Scenario: 10 consumers, 5 breaking changes, 20 usage events each.
    let pool = rt.block_on(bench_pool());
    let (diff_id, _) = rt.block_on(seed_blast_radius(&pool, 10, 5, 20));
    let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

    let mut group = c.benchmark_group("blast_radius");
    group.measurement_time(Duration::from_secs(15));
    // SLO: p95 < 300 ms — configure criterion's sample count accordingly.
    group.sample_size(50);

    group.bench_function("10consumers_5changes_20events", |b| {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        b.to_async(&rt).iter(|| {
            let app = app.clone();
            let uri = format!("/v1/diffs/{diff_id}/blast-radius");
            async move {
                let req = Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap();
                let resp = app.oneshot(req).await.unwrap();
                assert_eq!(resp.status(), 200);
            }
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 2 — Usage ingest throughput  (SLO: p99 < 100 ms for 100-event batch)
// ---------------------------------------------------------------------------

fn bench_usage_ingest(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let pool = rt.block_on(bench_pool());

    // Seed a consumer and service to satisfy the batch rows.
    let (consumer_id, service_id) = rt.block_on(async {
        let cid = Uuid::new_v4().to_string();
        let sid = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&cid)
        .bind("ingest-bench-consumer")
        .bind("")
        .bind("bench")
        .bind("")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&sid)
        .bind("ingest-bench-svc")
        .bind("")
        .bind("bench")
        .bind("openapi")
        .execute(&pool)
        .await
        .unwrap();
        let _ = now;
        (cid, sid)
    });

    let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

    let mut group = c.benchmark_group("usage_ingest");
    group.measurement_time(Duration::from_secs(15));
    group.sample_size(50);

    for batch_size in [10u32, 100, 500] {
        let body: Vec<serde_json::Value> = (0..batch_size)
            .map(|i| {
                serde_json::json!({
                    "consumer_id": consumer_id,
                    "service_id":  service_id,
                    "operation":   format!("GET /items/{i}"),
                    "field_path":  "id",
                })
            })
            .collect();
        let body_bytes = axum::body::Bytes::from(serde_json::to_vec(&body).unwrap());

        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &batch_size,
            |b, _| {
                use axum::body::Body;
                use axum::http::Request;
                use tower::ServiceExt;

                b.to_async(&rt).iter(|| {
                    let app = app.clone();
                    let bytes = body_bytes.clone();
                    async move {
                        let req = Request::builder()
                            .method("POST")
                            .uri("/v1/usage/events")
                            .header("content-type", "application/json")
                            .body(Body::from(bytes))
                            .unwrap();
                        let resp = app.oneshot(req).await.unwrap();
                        // 202 Accepted
                        assert_eq!(resp.status(), 202);
                    }
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------

criterion_group!(benches, bench_blast_radius, bench_usage_ingest);
criterion_main!(benches);
