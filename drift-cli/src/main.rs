use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use drift_core::models::Severity;

mod api_client;
mod claude;
mod explain;
mod github;
mod policy;
mod register;
mod render;

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

        /// UUID of the producer service (enables posting diff & fetching blast radius).
        #[arg(long)]
        service_id: Option<String>,

        /// Optional bearer token for the drift-api server.
        #[arg(long, env = "DRIFT_SERVICE_TOKEN")]
        token: Option<String>,
    },

    /// Register this service or consumer with the drift-api server.
    Register {
        /// Base URL of the drift-api server.
        #[arg(long, env = "DRIFT_API_URL")]
        api_url: String,

        /// UUID of the producer service to subscribe to.
        #[arg(long)]
        service_id: String,

        /// Name of this consumer.
        #[arg(long)]
        consumer_name: String,

        /// Repository URL of this consumer.
        #[arg(long)]
        repo_url: String,

        /// Owning team of this consumer.
        #[arg(long)]
        owner_team: String,

        /// Contact e-mail for this consumer.
        #[arg(long)]
        contact: String,

        /// Optional bearer token for the drift-api server.
        #[arg(long)]
        token: Option<String>,
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
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Check {
            base,
            head,
            spec: _spec,
            format: _format,
            policy,
            post_comment,
            json,
            no_color,
            api_url,
            service_id,
            token,
        } => {
            // Load policy
            let config = policy::load_config(policy.as_deref())?;
            let pol = config.policy();

            // Read specs — for P0, --base and --head are file paths
            let base_content = std::fs::read_to_string(&base)
                .map_err(|e| anyhow::anyhow!("cannot read base spec '{}': {e}", base))?;
            let head_content = std::fs::read_to_string(&head)
                .map_err(|e| anyhow::anyhow!("cannot read head spec '{}': {e}", head))?;

            let base_spec = drift_core::diff::parse_openapi(&base_content)
                .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
            let head_spec = drift_core::diff::parse_openapi(&head_content)
                .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;

            let changes = drift_core::diff::diff_openapi(&base_spec, &head_spec);

            let use_color = !no_color && std::env::var("NO_COLOR").is_err();

            if json {
                render::print_json(&changes);
            } else {
                render::print_table(&changes, use_color);
            }

            if post_comment {
                match github::GithubContext::from_env() {
                    Some(ctx) => {
                        let breaking = changes
                            .iter()
                            .filter(|c| c.severity == Severity::Breaking)
                            .count();
                        let policy_verdict = format!(
                            "Policy: `{}` \u{2014} {} breaking change(s) found.",
                            match pol.block_on {
                                policy::BlockOn::Never => "never",
                                policy::BlockOn::AnyBreak => "any_break",
                                policy::BlockOn::ActiveConsumers => "active_consumers",
                            },
                            breaking
                        );
                        let comment_body =
                            github::build_comment(&changes, &base, &head, &policy_verdict);
                        match github::post_or_update_comment(&ctx, &comment_body).await {
                            Ok(url) => println!("PR comment posted: {url}"),
                            Err(e) => eprintln!("Warning: failed to post PR comment: {e}"),
                        }
                    }
                    None => {
                        if !json {
                            eprintln!(
                                "Warning: --post-comment set but not running in a GitHub Actions PR context \
                                (GITHUB_TOKEN or PR number missing). Skipping."
                            );
                        }
                    }
                }
            }

            // Post diff and fetch blast radius when api_url and service_id are both set.
            if let (Some(ref url), Some(ref svc_id)) = (&api_url, &service_id) {
                let token_ref = token.as_deref();
                match api_client::post_diff(
                    url,
                    api_client::PostDiffParams {
                        service_id: svc_id,
                        service_name: svc_id,
                        from_ref: &base,
                        to_ref: &head,
                        pr_url: None,
                        spec_format: "openapi",
                        changes: &changes,
                        token: token_ref,
                    },
                )
                .await
                {
                    Ok(diff_id) => {
                        if !json {
                            println!("Diff posted: {diff_id}");
                        }
                        match api_client::get_blast_radius(url, &diff_id, token_ref).await {
                            Ok(br) => {
                                if !json {
                                    render::print_blast_radius(&br, use_color);
                                }
                            }
                            Err(e) => {
                                eprintln!("Warning: failed to fetch blast radius: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to post diff to API: {e}");
                    }
                }
            }

            // P0: no consumer registry yet → has_active_consumers = false
            let code = policy::exit_code(&changes, &pol, false);
            if code != 0 {
                std::process::exit(code);
            }
        }
        Commands::Register {
            api_url,
            service_id,
            consumer_name,
            repo_url,
            owner_team,
            contact,
            token,
        } => {
            register::run(
                &api_url,
                &service_id,
                &consumer_name,
                &repo_url,
                &owner_team,
                &contact,
                token.as_deref(),
            )
            .await?;
        }
        Commands::Scan { .. } => {
            println!("Not yet implemented: scan");
        }
        Commands::Explain {
            diff_id,
            release_notes,
            out,
            api_url,
        } => {
            explain::run(
                &api_url,
                &diff_id,
                release_notes,
                out.as_deref(),
                std::env::var("DRIFT_SERVICE_TOKEN").ok().as_deref(),
            )
            .await?;
        }
    }

    Ok(())
}
