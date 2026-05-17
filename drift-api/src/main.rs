use anyhow::Result;
use clap::Parser;
use tracing::info;

#[derive(Parser)]
#[command(
    name = "drift-api",
    version,
    about = "API Contract Drift Monitor — HTTP API server"
)]
struct Args {
    /// Database connection URL.
    /// Examples:
    ///   sqlite:drift.db
    ///   postgres://user:pass@host/dbname
    #[arg(long, env = "DATABASE_URL", default_value = "sqlite:drift.db")]
    db: String,

    /// Directory of pre-built static files to serve under /app (e.g. drift-ui/dist).
    #[arg(long, env = "STATIC_DIR")]
    static_dir: Option<String>,

    /// Socket address to listen on. Use 127.0.0.1:8080 in desktop sidecar mode.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8080")]
    bind: String,
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
    info!(db_url = %args.db, bind = %args.bind, "starting drift-api");

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
            match drift_api::purge_old_usage_events(&pool_for_retention, retention_days).await {
                Ok(n) => tracing::info!("retention: purged {n} old usage events"),
                Err(e) => tracing::warn!("retention job failed: {e}"),
            }
        }
    });

    drift_api::run(db_url, static_dir, bind_addr).await
}
