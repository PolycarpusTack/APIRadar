use anyhow::Result;
use clap::Parser;
use tracing::info;

const RETENTION_JOB_INTERVAL_SECS: u64 = 3600;
const RETENTION_DEFAULT_DAYS: u32 = 90;

/// Redact the `user:password@` portion of a database URL so the connection
/// string can be logged without leaking credentials. Inputs without userinfo
/// (e.g. `sqlite:drift.db`) are returned unchanged.
fn redact_db_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        if let Some((_userinfo, host_part)) = rest.split_once('@') {
            return format!("{scheme}://***@{host_part}");
        }
    }
    url.to_string()
}

#[cfg(test)]
mod redact_tests {
    use super::redact_db_url;

    #[test]
    fn redacts_postgres_userinfo() {
        assert_eq!(
            redact_db_url("postgres://user:s3cret@db.host:5432/drift"),
            "postgres://***@db.host:5432/drift"
        );
    }

    #[test]
    fn leaves_sqlite_unchanged() {
        assert_eq!(redact_db_url("sqlite:drift.db"), "sqlite:drift.db");
        assert_eq!(redact_db_url("sqlite::memory:"), "sqlite::memory:");
    }
}

#[derive(Parser)]
#[command(
    name = "radar-api",
    version,
    about = "API Contract Radar Monitor — HTTP API server"
)]
struct Args {
    /// Database connection URL.
    /// Examples:
    ///   sqlite:drift.db
    ///   postgres://user:pass@host/dbname
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite:drift.db")]
    db: String,

    /// Directory of pre-built static files to serve under /app (e.g. radar-ui/dist).
    #[arg(long, env = "STATIC_DIR")]
    static_dir: Option<String>,

    /// Socket address to listen on. Use 127.0.0.1:8080 in desktop sidecar mode.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8080")]
    bind: String,

    /// Maximum requests per minute per client IP (0 = unlimited).
    #[arg(long, env = "RATE_LIMIT_PER_MINUTE", default_value_t = 300)]
    rate_limit: u32,

    /// Maximum request body size in megabytes.
    #[arg(long, env = "MAX_BODY_SIZE_MB", default_value_t = 4)]
    max_body_size_mb: u32,
}

/// Does this bind address accept connections from outside this machine?
///
/// `0.0.0.0` was the only case the old warning caught; `[::]`, a LAN address
/// and a hostname are equally reachable. Anything that fails to parse is
/// treated as public, because guessing "local" on an address we do not
/// understand is the dangerous direction to be wrong in.
fn binds_publicly(bind: &str) -> bool {
    match bind.parse::<std::net::SocketAddr>() {
        Ok(addr) => !addr.ip().is_loopback(),
        Err(_) => true,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    info!(db_url = %redact_db_url(&args.db), bind = %args.bind, "starting radar-api");

    let require_auth = std::env::var("RADAR_REQUIRE_AUTH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    // F-09: refuse to serve an unauthenticated API on a reachable address.
    //
    // Auth can be switched on three different ways, and the middleware honours
    // all of them, so all three must count as "configured" here — checking
    // only RADAR_REQUIRE_AUTH would refuse to start deployments that are
    // perfectly well authenticated via a JWT secret or a service token.
    let auth_configured = require_auth
        || !std::env::var("RADAR_JWT_SECRET")
            .unwrap_or_default()
            .is_empty()
        || !std::env::var("RADAR_SERVICE_TOKEN")
            .unwrap_or_default()
            .is_empty();
    let allow_unauthenticated = std::env::var("RADAR_ALLOW_UNAUTHENTICATED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !auth_configured && binds_publicly(&args.bind) && !allow_unauthenticated {
        eprintln!(
            "REFUSING TO START: no authentication is configured and --bind {} is reachable \
             from outside this machine.\n\n\
             Pick one:\n  \
             * set RADAR_JWT_SECRET=<secret>        (OIDC / JWT sessions)\n  \
             * set RADAR_SERVICE_TOKEN=<token>      (static bearer token)\n  \
             * set RADAR_REQUIRE_AUTH=true          (reject unauthenticated requests)\n  \
             * pass --bind 127.0.0.1:<port>         (desktop / local-only)\n\n\
             To serve an unauthenticated API on purpose, set \
             RADAR_ALLOW_UNAUTHENTICATED=true.",
            args.bind
        );
        std::process::exit(2);
    }
    if !auth_configured && allow_unauthenticated {
        tracing::warn!(
            "SECURITY: serving an UNAUTHENTICATED API on {} because \
             RADAR_ALLOW_UNAUTHENTICATED is set.",
            args.bind
        );
    }

    let cors_origins = std::env::var("CORS_ALLOWED_ORIGINS").unwrap_or_default();
    if cors_origins.is_empty() && args.bind.starts_with("0.0.0.0") {
        tracing::warn!(
            "CORS: CORS_ALLOWED_ORIGINS is not set — all cross-origin requests are allowed on {}. \
             Set CORS_ALLOWED_ORIGINS=https://yourdomain.com for server deployments.",
            args.bind
        );
    }

    let db_url = radar_api::resolve_db_url(&args.db);
    let db_url = db_url.as_str();
    let static_dir = args.static_dir.as_deref();
    let bind_addr = args.bind.as_str();

    let pool_for_retention = {
        sqlx::any::install_default_drivers();
        sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(db_url)
            .await?
    };
    tokio::spawn(async move {
        let period = tokio::time::Duration::from_secs(RETENTION_JOB_INTERVAL_SECS);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        loop {
            interval.tick().await;
            // Read retention_days from settings each tick so UI changes take effect.
            let days: u32 =
                sqlx::query_scalar("SELECT value FROM settings WHERE key = 'retention.days'")
                    .fetch_optional(&pool_for_retention)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|v: String| v.parse().ok())
                    .unwrap_or(RETENTION_DEFAULT_DAYS);
            match radar_api::purge_old_usage_events(&pool_for_retention, days).await {
                Ok(n) => tracing::info!("retention: purged {n} old usage events (window={days}d)"),
                Err(e) => tracing::warn!("retention job failed: {e}"),
            }
            match radar_api::expire_old_evidence(&pool_for_retention).await {
                Ok(n) => {
                    tracing::info!("evidence expiry: removed {n} expired impact_evidence rows")
                }
                Err(e) => tracing::warn!("evidence expiry job failed: {e}"),
            }
            match radar_api::purge_old_csv_runs(&pool_for_retention, days).await {
                Ok(n) => tracing::info!("retention: purged {n} old csv run jobs (window={days}d)"),
                Err(e) => tracing::warn!("csv run retention job failed: {e}"),
            }
        }
    });

    let max_body_bytes = args.max_body_size_mb as usize * 1024 * 1024;
    radar_api::run(
        db_url,
        static_dir,
        bind_addr,
        args.rate_limit,
        max_body_bytes,
    )
    .await
}

#[cfg(test)]
mod bind_tests {
    use super::binds_publicly;

    #[test]
    fn loopback_is_not_public() {
        assert!(!binds_publicly("127.0.0.1:8080"));
        assert!(!binds_publicly("127.0.0.1:17380"));
        assert!(!binds_publicly("[::1]:8080"));
    }

    #[test]
    fn wildcard_is_public() {
        // The only case the old `starts_with("0.0.0.0")` check caught...
        assert!(binds_publicly("0.0.0.0:8080"));
        // ...and the one it missed entirely.
        assert!(binds_publicly("[::]:8080"));
    }

    #[test]
    fn lan_address_is_public() {
        assert!(binds_publicly("192.168.1.50:8080"));
        assert!(binds_publicly("10.0.0.5:8080"));
    }

    #[test]
    fn unparseable_is_treated_as_public() {
        // Fail safe: guessing "local" for an address we cannot parse is the
        // dangerous direction to be wrong in.
        assert!(binds_publicly("localhost:8080"));
        assert!(binds_publicly("not-an-address"));
        assert!(binds_publicly(""));
    }
}
