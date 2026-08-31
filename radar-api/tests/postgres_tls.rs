//! Proves that radar-api can reach PostgreSQL over TLS, and that certificate
//! verification is genuinely happening against the system trust store.
//!
//! Skipped unless `RADAR_TEST_PG_TLS_URL` is set, so local runs and the
//! non-TLS CI jobs stay green. The `rust-postgres-tls` CI job sets it to a
//! `sslmode=verify-full` URL pointing at a Postgres whose certificate is
//! signed by a CA installed in the OS trust store.
//!
//! Why this exists: sqlx moved from native-tls to
//! `tls-rustls-ring-native-roots`. The obvious `runtime-tokio-rustls`
//! shorthand would instead have selected webpki roots — a bundled Mozilla
//! store — silently breaking any deployment whose Postgres presents a private
//! or internal CA. Nothing else in the suite would notice that change.

use sqlx::Row;

fn tls_url() -> Option<String> {
    std::env::var("RADAR_TEST_PG_TLS_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

#[tokio::test]
async fn connects_over_tls_and_session_is_encrypted() {
    let Some(url) = tls_url() else {
        eprintln!("RADAR_TEST_PG_TLS_URL unset — skipping Postgres TLS test");
        return;
    };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("must connect to Postgres with sslmode=verify-full");

    // pg_stat_ssl reports whether THIS backend connection is using TLS. A
    // plaintext fallback would return false here rather than failing to
    // connect, so asserting on it is what makes the test meaningful.
    let row = sqlx::query("SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()")
        .fetch_one(&pool)
        .await
        .expect("pg_stat_ssl must be queryable");
    let ssl: bool = row.try_get("ssl").expect("ssl column");
    assert!(ssl, "connection reached Postgres but was NOT encrypted");

    let version = sqlx::query("SELECT version FROM pg_stat_ssl WHERE pid = pg_backend_pid()")
        .fetch_one(&pool)
        .await
        .and_then(|r| r.try_get::<Option<String>, _>("version"))
        .ok()
        .flatten()
        .unwrap_or_default();
    eprintln!("Postgres TLS session established: {version}");
    assert!(
        version.starts_with("TLSv1.3") || version.starts_with("TLSv1.2"),
        "unexpected TLS version: {version}"
    );
}

/// The negative half: pointed at a CA that did not sign the server's
/// certificate, `verify-full` must refuse the connection.
///
/// Without this, the positive test above could pass simply because
/// verification is being skipped. Using an unrelated CA (rather than a missing
/// file, or relying on the trust store's contents) makes the expected outcome
/// identical locally and in CI.
#[tokio::test]
async fn verify_full_rejects_a_certificate_from_an_untrusted_ca() {
    let (Some(base), Ok(bad_ca)) = (tls_url(), std::env::var("RADAR_TEST_PG_TLS_BAD_CA")) else {
        eprintln!("Postgres TLS env not set — skipping untrusted-CA test");
        return;
    };

    // Rebuild the URL from scratch so there is exactly one sslrootcert.
    let stripped = base.split('?').next().unwrap_or(&base);
    let url = format!("{stripped}?sslmode=verify-full&sslrootcert={bad_ca}");

    let result = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await;

    match result {
        Ok(_) => panic!("verify-full accepted a certificate from an untrusted CA"),
        Err(e) => eprintln!("correctly rejected untrusted CA: {e}"),
    }
}
