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

    let db_url = args.db.as_str();
    let static_dir = args.static_dir.as_deref();
    let bind_addr = args.bind.as_str();

    let pool_for_retention = {
        sqlx::any::install_default_drivers();
        sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(db_url)
            .await?
    };
    let retention_days = 90u32;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match radar_api::purge_old_usage_events(&pool_for_retention, retention_days).await {
                Ok(n) => tracing::info!("retention: purged {n} old usage events"),
                Err(e) => tracing::warn!("retention job failed: {e}"),
            }
        }
    });

    let max_body_bytes = args.max_body_size_mb as usize * 1024 * 1024;
    radar_api::run(db_url, static_dir, bind_addr, args.rate_limit, max_body_bytes).await
}
