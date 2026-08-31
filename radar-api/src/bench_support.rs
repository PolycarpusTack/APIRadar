//! Seeding helpers shared by the SLO benchmarks and the SLO guard test.
//!
//! These live in the library rather than in the bench so that
//! `tests/slo_guard.rs` measures exactly the scenario `benches/slo.rs`
//! benchmarks. If the two drifted apart, the guard would stop guarding the
//! thing the SLO is stated about.
//!
//! Not part of the public API — `#[doc(hidden)]`, present only because Rust
//! benches can link a library but cannot reach into `tests/` or `#[cfg(test)]`.

use sqlx::AnyPool;
use uuid::Uuid;

/// In-memory SQLite pool with migrations applied.
#[doc(hidden)]
pub async fn bench_pool() -> AnyPool {
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

/// Seed a service, two spec versions, a diff with `n_changes` changes, and
/// `n_consumers` consumers each with `events_per_consumer` usage events.
/// Returns `(diff_id, service_id)`.
#[doc(hidden)]
pub async fn seed_blast_radius(
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
