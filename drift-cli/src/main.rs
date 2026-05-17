use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "drift",
    version,
    about = "API Contract Drift Monitor — CLI client",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compare two spec versions and report breaking changes.
    Check {
        /// Base git ref (commit / branch / tag) or spec file path.
        #[arg(long)]
        base: String,

        /// Head git ref (commit / branch / tag) or spec file path.
        #[arg(long)]
        head: String,

        /// Path to a spec file (overrides git-based resolution).
        #[arg(long)]
        spec: Option<String>,

        /// Spec format: openapi | graphql | protobuf.
        #[arg(long)]
        format: Option<String>,

        /// Path to a policy file (TOML/YAML) controlling severity thresholds.
        #[arg(long)]
        policy: Option<PathBuf>,

        /// Post a comment to the pull request if in CI.
        #[arg(long, default_value_t = false)]
        post_comment: bool,

        /// Emit machine-readable JSON output.
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Disable ANSI colour codes in output.
        #[arg(long, default_value_t = false)]
        no_color: bool,

        /// Base URL of the drift-api server.
        #[arg(long, env = "DRIFT_API_URL")]
        api_url: Option<String>,
    },

    /// Register this service or consumer with the drift-api server.
    Register {
        /// Base URL of the drift-api server.
        #[arg(long, env = "DRIFT_API_URL")]
        api_url: String,
    },

    /// Scan a consumer repository for API usage.
    Scan {
        /// UUID of the consumer to scan.
        #[arg(long)]
        consumer_id: String,

        /// Base URL of the drift-api server.
        #[arg(long, env = "DRIFT_API_URL")]
        api_url: String,
    },

    /// Explain the impact of a diff and optionally generate release notes.
    Explain {
        /// UUID of the diff to explain.
        #[arg(long)]
        diff_id: String,

        /// Generate human-readable release notes.
        #[arg(long, default_value_t = false)]
        release_notes: bool,

        /// Write output to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,

        /// Base URL of the drift-api server.
        #[arg(long, env = "DRIFT_API_URL")]
        api_url: String,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Check { .. } => {
            info!("Not yet implemented: check");
            println!("Not yet implemented: check");
        }
        Commands::Register { .. } => {
            info!("Not yet implemented: register");
            println!("Not yet implemented: register");
        }
        Commands::Scan { .. } => {
            info!("Not yet implemented: scan");
            println!("Not yet implemented: scan");
        }
        Commands::Explain { .. } => {
            info!("Not yet implemented: explain");
            println!("Not yet implemented: explain");
        }
    }

    Ok(())
}
