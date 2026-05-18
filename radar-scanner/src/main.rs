use anyhow::Result;
use std::path::Path;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("radar-scanner: starting");

    // One-shot scan of SOURCE_DIR when provided; otherwise idle heartbeat.
    let source_dir = std::env::var("SOURCE_DIR").ok();

    if let Some(dir) = source_dir {
        let path = Path::new(&dir);
        info!("scanning {}", path.display());
        let records = radar_scanner::scan_directory(path);
        info!("found {} call site records", records.len());
        for rec in &records {
            info!("  {}:{} → {}", rec.file_path, rec.line_number, rec.field_path);
        }
    } else {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            info!("radar-scanner: heartbeat (set SOURCE_DIR to enable scanning)");
        }
    }

    Ok(())
}
