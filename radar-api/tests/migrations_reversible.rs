//! Every migration must be reversible, and the reversal must actually run.
//!
//! Until v0.3.0 the directory used sqlx's simple (forward-only) format: 34
//! migrations, zero down-files. There was no way back from a bad migration in
//! anyone's environment, which is a poor property for software that asks teams
//! to point production CI at it.
//!
//! A down-migration that has never been executed is worth very little, so this
//! test migrates all the way up and then reverts all the way back down,
//! asserting the schema is empty at the end. It runs on SQLite; the Postgres
//! job exercises the same files against a real server.

use sqlx::migrate::Migrator;
use sqlx::Row;
use std::path::Path;

async fn user_table_count(pool: &sqlx::AnyPool) -> i64 {
    // `_sqlx_migrations` is sqlx's own bookkeeping and survives a full revert.
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await
    .expect("count tables");
    row.try_get::<i64, _>("n").expect("n")
}

#[tokio::test]
async fn every_migration_can_be_applied_and_reverted() {
    sqlx::any::install_default_drivers();
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");

    // SQLite defaults foreign_keys=OFF, which would let a wrong DROP TABLE
    // order pass here and then fail on PostgreSQL, where the constraint is
    // always enforced. Turn it on so the revert order is genuinely tested.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .expect("enable foreign keys");

    let migrator = Migrator::new(Path::new("./migrations"))
        .await
        .expect("load migrations");

    // Every migration must declare a reversal — a forward-only file silently
    // reintroduces the gap this test exists to close.
    // iter() yields both halves of each pair, so look only at the up side:
    // a Simple (forward-only) migration is the thing to catch.
    let not_reversible: Vec<_> = migrator
        .iter()
        .filter(|m| m.migration_type.is_up_migration() && !m.migration_type.is_reversible())
        .map(|m| format!("{} {}", m.version, m.description))
        .collect();
    assert!(
        not_reversible.is_empty(),
        "these migrations are not reversible: {not_reversible:?}"
    );

    migrator.run(&pool).await.expect("migrate up");
    let applied = user_table_count(&pool).await;
    assert!(
        applied > 20,
        "expected a populated schema, saw {applied} tables"
    );

    // undo(target) reverts everything with version > target, newest first, so
    // -1 unwinds the entire history in one pass.
    migrator
        .undo(&pool, -1)
        .await
        .expect("every down-migration must run");

    let remaining = user_table_count(&pool).await;
    assert_eq!(
        remaining, 0,
        "reverting every migration should leave no user tables, found {remaining}"
    );
}
