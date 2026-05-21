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
use radar_api::build_router;
use sqlx::AnyPool;
use std::time::Duration;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared test-pool helper (mirrors the unit test version)
// ---------------------------------------------------------------------------

async fn bench_pool() -> AnyPool {
    sqlx::any::install_default_drivers();
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("bench pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Insert `n_consumers` consumers subscribed to one service, plus one diff
/// with `n_changes` field-level breaking changes, and `events_per_consumer`
/// runtime usage events per consumer. Returns `(diff_id, service_id)`.
async fn seed_blast_radius(
    pool: &AnyPool,
    n_consumers: usize,
    n_changes: usize,
    events_per_consumer: usize,
) -> (String, String) {
    let now = chrono::Utc::now().to_rfc3339();

    let service_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO service (id, name, repo_url, owner_team, spec_format) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&service_id)
    .bind("bench-svc")
    .bind("")
    .bind("bench")
    .bind("openapi")
    .execute(pool)
    .await
    .unwrap();

    let from_sv = Uuid::new_v4().to_string();
    let to_sv = Uuid::new_v4().to_string();
    for (id, git_ref) in [(&from_sv, "v1.0"), (&to_sv, "v1.1")] {
        sqlx::query(
            "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&service_id)
        .bind(git_ref)
        .bind(&now)
        .bind("openapi")
        .execute(pool)
        .await
        .unwrap();
    }

    let diff_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&diff_id)
    .bind(&from_sv)
    .bind(&to_sv)
    .bind::<Option<String>>(None)
    .bind(&now)
    .execute(pool)
    .await
    .unwrap();

    for i in 0..n_changes {
        let cid = Uuid::new_v4().to_string();
        let path = format!("GET /items \u{2192} response.field{i}");
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&cid)
        .bind(&diff_id)
        .bind(&path)
        .bind("field_removed")
        .bind("breaking")
        .bind::<Option<String>>(None)
        .execute(pool)
        .await
        .unwrap();
    }

    for c in 0..n_consumers {
        let cid = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&cid)
        .bind(format!("consumer-{c}"))
        .bind("")
        .bind("bench")
        .bind("")
        .execute(pool)
        .await
        .unwrap();

        let sub_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO subscription (id, service_id, consumer_id, opted_in_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&sub_id)
        .bind(&service_id)
        .bind(&cid)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        for e in 0..events_per_consumer {
            let eid = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO usage_event (id, consumer_id, service_id, operation, field_path, recorded_at) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&eid)
            .bind(&cid)
            .bind(&service_id)
            .bind("GET /items")
            .bind(format!("field{}", e % n_changes.max(1)))
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    (diff_id, service_id)
}

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
        let body_bytes =
            axum::body::Bytes::from(serde_json::to_vec(&body).unwrap());

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
