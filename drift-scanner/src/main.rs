use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("drift-scanner: not yet implemented");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        info!("drift-scanner: heartbeat (placeholder — worker not yet implemented)");
    }
}
