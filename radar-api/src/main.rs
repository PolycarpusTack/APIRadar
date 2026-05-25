use anyhow::Result;
use clap::Parser;
use tracing::info;

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
    info!(db_url = %args.db, bind = %args.bind, "starting radar-api");

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
        let period = tokio::time::Duration::from_secs(3600);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        loop {
            interval.tick().await;
            // Read retention_days from settings each tick so UI changes take effect.
            let days: u32 = sqlx::query_scalar(
                "SELECT value FROM settings WHERE key = 'retention.days'",
            )
            .fetch_optional(&pool_for_retention)
            .await
            .ok()
            .flatten()
            .and_then(|v: String| v.parse().ok())
            .unwrap_or(90);
            match radar_api::purge_old_usage_events(&pool_for_retention, days).await {
                Ok(n) => tracing::info!("retention: purged {n} old usage events (window={days}d)"),
                Err(e) => tracing::warn!("retention job failed: {e}"),
            }
            match radar_api::expire_old_evidence(&pool_for_retention).await {
                Ok(n) => tracing::info!("evidence expiry: removed {n} expired impact_evidence rows"),
                Err(e) => tracing::warn!("evidence expiry job failed: {e}"),
            }
        }
    });

    let max_body_bytes = args.max_body_size_mb as usize * 1024 * 1024;
    radar_api::run(db_url, static_dir, bind_addr, args.rate_limit, max_body_bytes).await
}
