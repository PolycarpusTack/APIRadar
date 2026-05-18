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

        /// UUID of the producer service whose fields to track.
        #[arg(long)]
        service_id: String,

        /// Directory to scan for TypeScript, Python, and Go source files.
        #[arg(long)]
        source_dir: PathBuf,

        /// Base URL of the drift-api server.
        #[arg(long, env = "DRIFT_API_URL")]
        api_url: String,

        /// Optional bearer token for the drift-api server.
        #[arg(long, env = "DRIFT_SERVICE_TOKEN")]
        token: Option<String>,
    },

    /// Explain the impact of a diff and optionally generate release notes.
    Explain {
        /// UUID of the diff to explain.
        #[arg(long)]
        diff_id: String,

        /// Generate human-readable release notes (Markdown).
        #[arg(long, default_value_t = false)]
        release_notes: bool,

        /// Generate per-consumer migration guides using Claude (requires ANTHROPIC_API_KEY).
        #[arg(long, default_value_t = false)]
        migration_guide: bool,

        /// Post the release notes as a GitHub Release (requires GITHUB_TOKEN).
        #[arg(long, default_value_t = false)]
        post_github_release: bool,

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
            format,
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

            // Read specs — --base and --head are file paths
            let base_content = std::fs::read_to_string(&base)
                .map_err(|e| anyhow::anyhow!("cannot read base spec '{}': {e}", base))?;
            let head_content = std::fs::read_to_string(&head)
                .map_err(|e| anyhow::anyhow!("cannot read head spec '{}': {e}", head))?;

            // Detect format: explicit flag wins, then file extension of head path
            let detected = format
                .as_deref()
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| detect_format(&head));

            let changes = match detected.as_str() {
                "graphql" | "gql" => {
                    let base_map = drift_core::graphql::parse_graphql(&base_content)
                        .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
                    let head_map = drift_core::graphql::parse_graphql(&head_content)
                        .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;
                    drift_core::graphql::diff_graphql(&base_map, &head_map)
                }
                "protobuf" | "proto" => {
                    let base_schema = drift_core::proto::parse_proto(&base_content)
                        .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
                    let head_schema = drift_core::proto::parse_proto(&head_content)
                        .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;
                    drift_core::proto::diff_proto(&base_schema, &head_schema)
                }
                _ => {
                    // Default: OpenAPI
                    let base_spec = drift_core::diff::parse_openapi(&base_content)
                        .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
                    let head_spec = drift_core::diff::parse_openapi(&head_content)
                        .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;
                    drift_core::diff::diff_openapi(&base_spec, &head_spec)
                }
            };

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
            let mut has_active_consumers = false;
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
                                has_active_consumers = !br.entries.is_empty();
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

            // D-2: check if the PR carries the configured label override.
            let has_label_override =
                if let Some(ref override_cfg) = pol.allow_override_with {
                    if let Some(label) = override_cfg.strip_prefix("label:") {
                        match github::GithubContext::from_env() {
                            Some(ctx) => github::pr_has_label(&ctx, label).await,
                            None => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

            let code = policy::exit_code(&changes, &pol, has_active_consumers, has_label_override);
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
        Commands::Scan {
            consumer_id,
            service_id,
            source_dir,
            api_url,
            token,
        } => {
            println!("Scanning {}…", source_dir.display());
            let records = drift_scanner::scan_directory(&source_dir);
            println!("Found {} property accesses.", records.len());

            if records.is_empty() {
                return Ok(());
            }

            let sites: Vec<api_client::CallSiteBody> = records
                .into_iter()
                .map(|r| api_client::CallSiteBody {
                    consumer_id: consumer_id.clone(),
                    service_id: service_id.clone(),
                    operation: String::new(),
                    file_path: r.file_path,
                    line_number: r.line_number as i64,
                    field_path: r.field_path,
                })
                .collect();

            // Post in chunks of 500 to stay within server limit.
            let mut total = 0usize;
            for chunk in sites.chunks(500) {
                match api_client::post_call_sites(&api_url, chunk, token.as_deref()).await {
                    Ok(n) => total += n,
                    Err(e) => eprintln!("Warning: failed to post call sites: {e}"),
                }
            }
            println!("Posted {total} call site record(s) to {api_url}.");
        }
        Commands::Explain {
            diff_id,
            release_notes,
            migration_guide,
            post_github_release,
            out,
            api_url,
        } => {
            explain::run(
                &api_url,
                &diff_id,
                release_notes,
                migration_guide,
                post_github_release,
                out.as_deref(),
                std::env::var("DRIFT_SERVICE_TOKEN").ok().as_deref(),
            )
            .await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Format auto-detection
// ---------------------------------------------------------------------------

fn detect_format(path: &str) -> String {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "graphql" | "gql" => "graphql".to_string(),
        "proto" => "protobuf".to_string(),
        _ => "openapi".to_string(),
    }
}
