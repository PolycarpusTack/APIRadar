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

/// The negative half: `verify-full` must reject a certificate whose SAN does
/// not cover the host we asked for.
///
/// Two earlier attempts at this test were wrong, and both were wrong in the
/// direction that would have made the positive test above meaningless:
///
///   1. Pointing `sslrootcert` at a nonexistent file — undefined once the URL
///      already carries an `sslrootcert`.
///   2. Pointing `sslrootcert` at a real-but-unrelated CA — this still
///      CONNECTS whenever the genuine CA is present in the OS trust store,
///      because `sslrootcert` *augments* the trust anchors rather than
///      replacing them. Verified against a live server: it is not a way to
///      simulate an untrusted chain.
///
/// Hostname mismatch is independent of what happens to be trusted, so it
/// behaves identically on a developer machine and on a CI runner where the
/// test CA has been installed system-wide.
#[tokio::test]
async fn verify_full_rejects_a_hostname_the_certificate_does_not_cover() {
    let Ok(url) = std::env::var("RADAR_TEST_PG_TLS_WRONGHOST_URL") else {
        eprintln!("RADAR_TEST_PG_TLS_WRONGHOST_URL unset — skipping hostname-mismatch test");
        return;
    };

    let result = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await;

    match result {
        Ok(_) => panic!(
            "verify-full accepted a certificate that does not cover the requested host — \
             certificate verification is not actually happening"
        ),
        Err(e) => eprintln!("correctly rejected hostname mismatch: {e}"),
    }
}
