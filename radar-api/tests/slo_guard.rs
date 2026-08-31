//! Enforces the SLO targets that `benches/slo.rs` measures.
//!
//! The benches declared these numbers from the start:
//!
//!     blast-radius query   p95 < 300 ms
//!     usage ingest batch   p99 < 100 ms  (batch of 100)
//!
//! ...but nothing ever ran them, so the targets were aspirational — a
//! regression could land and no gate would notice. Criterion measures rather
//! than asserts, so this test does the asserting.
//!
//! **These assert only in an optimized build.** An SLO is a statement about
//! production, and production is a release build; asserting a production bound
//! against unoptimized code compares the wrong things. Measured on this
//! hardware:
//!
//!     release (what the SLO is about)      debug (what `cargo test` builds)
//!     blast-radius p95   4.19 ms            24.1 ms
//!     ingest p99        16.2  ms            40.0 ms
//!
//! Debug leaves the ingest bound only ~2.5x of headroom, which a slow CI
//! runner could plausibly exceed — a flaky gate is worse than no gate. So in a
//! debug build these measure and report without failing, and CI runs them with
//! `--release`, where the headroom is 71x and 6x respectively.
//!
//! If the release assertion ever fires, the honest fix is to investigate the
//! regression — not to raise the bound.

/// Whether to enforce, or merely report. See the module docs.
fn enforcing() -> bool {
    !cfg!(debug_assertions)
}

/// Report the measurement, and assert it only where the assertion is meaningful.
fn check(label: &str, measured: Duration, slo: Duration) {
    let mode = if enforcing() {
        "enforced"
    } else {
        "reporting only"
    };
    eprintln!("{label}: {measured:?} (SLO {slo:?}, {mode})");
    if enforcing() {
        assert!(
            measured < slo,
            "{label} regressed to {measured:?}, over the {slo:?} SLO"
        );
    }
}

use axum::body::Body;
use axum::http::Request;
use radar_api::bench_support::{bench_pool, seed_blast_radius};
use radar_api::build_router;
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// Nth-percentile of a set of samples, nearest-rank.
fn percentile(mut samples: Vec<Duration>, p: f64) -> Duration {
    samples.sort_unstable();
    let rank = ((p / 100.0) * samples.len() as f64).ceil() as usize;
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

#[tokio::test]
async fn blast_radius_p95_stays_under_300ms() {
    const SLO: Duration = Duration::from_millis(300);
    const SAMPLES: usize = 50;

    let pool = bench_pool().await;
    let (diff_id, _) = seed_blast_radius(&pool, 10, 5, 20).await;
    let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let req = Request::builder()
            .method("GET")
            .uri(format!("/v1/diffs/{diff_id}/blast-radius"))
            .body(Body::empty())
            .unwrap();
        let start = Instant::now();
        let resp = app.clone().oneshot(req).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), 200, "blast-radius must succeed to be timed");
        samples.push(elapsed);
    }

    let p95 = percentile(samples.clone(), 95.0);
    let median = percentile(samples, 50.0);
    eprintln!("blast-radius median {median:?}");
    check("blast-radius p95", p95, SLO);
}

#[tokio::test]
async fn usage_ingest_p99_stays_under_100ms_for_a_batch_of_100() {
    const SLO: Duration = Duration::from_millis(100);
    const SAMPLES: usize = 30;
    const BATCH: usize = 100;

    let pool = bench_pool().await;
    let (_, service_id) = seed_blast_radius(&pool, 1, 1, 1).await;

    // The batch rows carry a foreign key to `consumer`, so seed one we can name.
    let consumer_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO consumer (id, name, repo_url, owner_team, contact) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&consumer_id)
    .bind("slo-guard-consumer")
    .bind("")
    .bind("bench")
    .bind("")
    .execute(&pool)
    .await
    .expect("seed consumer");

    let app = build_router(pool, None, 4 * 1024 * 1024, false, None);

    // A bare JSON array, matching POST /v1/usage/events.
    let body = serde_json::to_vec(
        &(0..BATCH)
            .map(|i| {
                serde_json::json!({
                    "consumer_id": consumer_id,
                    "service_id": service_id,
                    "operation": "GET /items",
                    "field_path": format!("response.field{i}"),
                })
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();

    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/usage/events")
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .unwrap();
        let start = Instant::now();
        let resp = app.clone().oneshot(req).await.unwrap();
        let elapsed = start.elapsed();
        assert!(
            resp.status().is_success(),
            "usage ingest must succeed to be timed, got {}",
            resp.status()
        );
        samples.push(elapsed);
    }

    let p99 = percentile(samples.clone(), 99.0);
    let median = percentile(samples, 50.0);
    eprintln!("usage-ingest(100) median {median:?}");
    check("usage-ingest(100) p99", p99, SLO);
}
