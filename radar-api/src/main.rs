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

    // Warn when auth is disabled and the server is binding to all interfaces.
    // In desktop sidecar mode, pass --bind 127.0.0.1:8080 to suppress this warning.
    let require_auth = std::env::var("RADAR_REQUIRE_AUTH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !require_auth && args.bind.starts_with("0.0.0.0") {
        tracing::warn!(
            "SECURITY: RADAR_REQUIRE_AUTH is not set — API is unauthenticated on {}. \
             Set RADAR_REQUIRE_AUTH=true or pass --bind 127.0.0.1:<port> for local-only access.",
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
