use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

mod ai_provider;
mod api_client;
mod apitesting;
mod claude;
mod explain;
mod github;
mod jira;
mod policy;
mod postman;
mod register;
mod render;
mod test_gen;

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum RuleAction {
    /// Create a new evolution rule.
    Add {
        /// Human-readable label for this rule.
        #[arg(long)]
        name: String,
        /// ChangeKind to target (e.g. field_added, enum_value_added).
        #[arg(long)]
        change_kind: String,
        /// Optional dot-separated field path glob (e.g. "users.*", "**.legacy_field").
        #[arg(long)]
        path_pattern: Option<String>,
        /// Target severity: safe | non_breaking_risky.
        #[arg(long)]
        severity_override: String,
        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: String,
        /// Optional bearer token.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,
    },
    /// List all evolution rules for this org.
    List {
        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: String,
        /// Optional bearer token.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,
    },
    /// Delete an evolution rule by ID.
    Delete {
        /// Rule ID to delete.
        id: String,
        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: String,
        /// Optional bearer token.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,
    },
    /// Enable or disable an evolution rule.
    Toggle {
        /// Rule ID.
        id: String,
        /// Set to true to enable, false to disable.
        #[arg(long)]
        enabled: bool,
        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: String,
        /// Optional bearer token.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,
    },
    /// Show which evolution rules would apply to a specific diff.
    Test {
        /// Diff ID to test rules against.
        #[arg(long)]
        diff_id: String,
        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: String,
        /// Optional bearer token.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,
    },
}

#[derive(Parser)]
#[command(
    name = "radar",
    version,
    about = "API Contract Radar Monitor — CLI client",
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

        /// Write a JSON summary file (diff_id, breaking_count, policy_verdict, …) to this path.
        /// Consumed by radar-action to set GitHub Action outputs.
        #[arg(long)]
        summary_file: Option<PathBuf>,

        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: Option<String>,

        /// UUID of the producer service (enables posting diff & fetching blast radius).
        #[arg(long)]
        service_id: Option<String>,

        /// Optional bearer token for the radar-api server.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,
    },

    /// Register this service or consumer with the radar-api server.
    Register {
        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
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

        /// Optional bearer token for the radar-api server.
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

        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
        api_url: String,

        /// Optional bearer token for the radar-api server.
        #[arg(long, env = "RADAR_SERVICE_TOKEN")]
        token: Option<String>,

        /// Map property names to API operations for blast-radius evidence.
        /// Format: "field=METHOD /path" — can be repeated.
        /// Example: --operation-map "userId=GET /users" --operation-map "email=GET /users"
        #[arg(long, value_name = "FIELD=OP")]
        operation_map: Vec<String>,

        /// Postman Collection v2.1 JSON files to scan for Consumer evidence.
        /// Can be repeated. These auto-register the consumer and post evidence to the API.
        /// Example: --collection collections/payments.postman_collection.json
        #[arg(long, value_name = "PATH")]
        collection: Vec<PathBuf>,
    },

    /// Generate Postman test cases from a Jira ticket and an OpenAPI spec.
    GenerateTests {
        /// Jira ticket key (e.g. PROJ-123). Reads JIRA_BASE_URL, JIRA_EMAIL, JIRA_TOKEN.
        #[arg(long)]
        jira: Option<String>,

        /// Paste Jira ticket text directly (fallback when --jira or Jira credentials are absent).
        #[arg(long)]
        jira_text: Option<String>,

        /// Path to the OpenAPI YAML/JSON spec file.
        #[arg(long)]
        spec: Option<std::path::PathBuf>,

        /// Base URL inserted into every generated request (default: http://localhost:8080).
        #[arg(long, default_value = "http://localhost:8080")]
        base_url: String,

        /// Write the Postman Collection JSON to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,

        /// Push the collection to this Postman workspace ID (requires POSTMAN_API_KEY).
        #[arg(long)]
        postman_workspace: Option<String>,
    },

    /// Manage evolution rules — org-level severity overrides for specific change kinds.
    Rule {
        #[command(subcommand)]
        action: RuleAction,
    },

    /// Print shell completion script to stdout.
    Completions {
        /// Shell to generate completions for: bash, zsh, fish, powershell, elvish.
        #[arg(value_enum)]
        shell: Shell,
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

        /// Base URL of the radar-api server.
        #[arg(long, env = "RADAR_API_URL")]
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
            summary_file,
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
                    let base_map = radar_core::graphql::parse_graphql(&base_content)
                        .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
                    let head_map = radar_core::graphql::parse_graphql(&head_content)
                        .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;
                    radar_core::graphql::diff_graphql(&base_map, &head_map)
                }
                "protobuf" | "proto" => {
                    let base_schema = radar_core::proto::parse_proto(&base_content)
                        .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
                    let head_schema = radar_core::proto::parse_proto(&head_content)
                        .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;
                    radar_core::proto::diff_proto(&base_schema, &head_schema)
                }
                _ => {
                    // Default: OpenAPI
                    let base_spec = radar_core::diff::parse_openapi(&base_content)
                        .map_err(|e| anyhow::anyhow!("parse error in base spec: {e}"))?;
                    let head_spec = radar_core::diff::parse_openapi(&head_content)
                        .map_err(|e| anyhow::anyhow!("parse error in head spec: {e}"))?;
                    radar_core::diff::diff_openapi(&base_spec, &head_spec)
                }
            };

            let use_color = !no_color && std::env::var("NO_COLOR").is_err();

            if json {
                render::print_json(&changes);
            } else {
                render::print_table(&changes, use_color);
            }

            // Post diff and fetch blast radius when api_url and service_id are both set.
            let mut has_active_consumers = false;
            let mut api_error = false;
            let mut posted_diff_id: Option<String> = None;
            let mut blast_radius_data: Option<render::BlastRadiusResponse> = None;

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
                                blast_radius_data = Some(br);
                            }
                            Err(e) => {
                                eprintln!("Warning: failed to fetch blast radius: {e}");
                                api_error = true;
                            }
                        }
                        posted_diff_id = Some(diff_id);
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to post diff to API: {e}");
                        api_error = true;
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

            // F-3: check for a server-side acknowledgement (overrides block verdict).
            let has_label_override = if !has_label_override {
                if let (Some(ref url), Some(ref did)) = (&api_url, &posted_diff_id) {
                    api_client::check_diff_acknowledged(url, did, token.as_deref())
                        .await
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                has_label_override
            };

            // E-3: compute policy verdict using fail_mode semantics.
            let fail_mode = config.fail_mode();
            let decision =
                policy::decide(&changes, &pol, &fail_mode, has_active_consumers, has_label_override, api_error);

            if decision.verdict == policy::Verdict::Warn && !json {
                eprintln!(
                    "Warning: drift check completed with verdict=warn (fail_mode={:?}) — build not blocked.",
                    decision.fail_mode
                );
            }

            // Post policy decision to radar-api when api_url is set.
            if let Some(ref url) = api_url {
                let verdict_str = match &decision.verdict {
                    policy::Verdict::Pass => "pass",
                    policy::Verdict::Warn => "warn",
                    policy::Verdict::Block => "block",
                    policy::Verdict::Overridden => "overridden",
                };
                let fm_str = match &decision.fail_mode {
                    policy::FailMode::Closed => "closed",
                    policy::FailMode::Open => "open",
                    policy::FailMode::Warn => "warn",
                };
                if let Err(e) = api_client::post_policy_decision(
                    url,
                    posted_diff_id.as_deref(),
                    service_id.as_deref(),
                    verdict_str,
                    fm_str,
                    "radar-cli",
                    token.as_deref(),
                )
                .await
                {
                    eprintln!("Warning: failed to post policy decision: {e}");
                }
            }

            // Post PR comment with evidence table and policy verdict (E-4).
            // Runs after blast-radius and policy decision are both available.
            if post_comment {
                match github::GithubContext::from_env() {
                    Some(ctx) => {
                        let verdict_str = match &decision.verdict {
                            policy::Verdict::Pass => "pass",
                            policy::Verdict::Warn => "warn",
                            policy::Verdict::Block => "block",
                            policy::Verdict::Overridden => "overridden",
                        };
                        let fm_str = match &decision.fail_mode {
                            policy::FailMode::Closed => "closed",
                            policy::FailMode::Open => "open",
                            policy::FailMode::Warn => "warn",
                        };
                        // H-5: fetch test suites generated for this diff.
                        let test_suites: Vec<github::TestSuiteSummary> =
                            if let (Some(ref url), Some(ref did)) = (&api_url, &posted_diff_id) {
                                api_client::fetch_diff_test_suites(url, did, token.as_deref())
                                    .await
                                    .unwrap_or_default()
                            } else {
                                vec![]
                            };
                        let comment_body = github::build_comment_with_suites(
                            &changes,
                            &base,
                            &head,
                            blast_radius_data.as_ref(),
                            verdict_str,
                            fm_str,
                            &test_suites,
                        );
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

            // Write machine-readable summary for radar-action output parsing.
            if let Some(ref sf) = summary_file {
                let breaking_count = changes
                    .iter()
                    .filter(|c| c.severity == radar_core::models::Severity::Breaking)
                    .count();
                let affected_consumer_count = blast_radius_data
                    .as_ref()
                    .map(|br| br.entries.len())
                    .unwrap_or(0);
                let verdict_str = match &decision.verdict {
                    policy::Verdict::Pass => "pass",
                    policy::Verdict::Warn => "warn",
                    policy::Verdict::Block => "block",
                    policy::Verdict::Overridden => "overridden",
                };
                let dashboard_url = if let (Some(ref url), Some(ref did)) = (&api_url, &posted_diff_id) {
                    Some(format!("{url}/app/diffs/{did}"))
                } else {
                    None
                };
                let summary = render::CheckSummary {
                    diff_id: posted_diff_id.clone(),
                    breaking_count,
                    affected_consumer_count,
                    policy_verdict: verdict_str.to_string(),
                    dashboard_url,
                };
                if let Err(e) = render::write_summary(sf, &summary) {
                    eprintln!("Warning: failed to write summary file: {e}");
                }
            }

            if decision.exit_code != 0 {
                std::process::exit(decision.exit_code);
            }
        }
        Commands::Rule { action } => {
            match action {
                RuleAction::Add { name, change_kind, path_pattern, severity_override, api_url, token } => {
                    let body = api_client::CreateRuleBody { name, change_kind, path_pattern, severity_override };
                    let rule = api_client::create_evolution_rule(&api_url, &body, token.as_deref()).await?;
                    println!("Created rule: {}", rule["id"].as_str().unwrap_or("?"));
                    println!("  kind:     {}", rule["change_kind"].as_str().unwrap_or("?"));
                    println!("  override: {}", rule["severity_override"].as_str().unwrap_or("?"));
                    if let Some(p) = rule["path_pattern"].as_str() {
                        println!("  pattern:  {p}");
                    }
                }
                RuleAction::List { api_url, token } => {
                    let rules = api_client::list_evolution_rules(&api_url, token.as_deref()).await?;
                    if rules.is_empty() {
                        println!("No evolution rules configured.");
                    } else {
                        println!("{:<38} {:<24} {:<22} {:<12} Pattern", "ID", "Name", "ChangeKind", "Override");
                        println!("{}", "-".repeat(110));
                        for r in &rules {
                            let enabled = r["enabled"].as_bool().unwrap_or(true);
                            let status = if enabled { "" } else { " [disabled]" };
                            println!(
                                "{:<38} {:<24} {:<22} {:<12} {}{}",
                                r["id"].as_str().unwrap_or("?"),
                                r["name"].as_str().unwrap_or("?"),
                                r["change_kind"].as_str().unwrap_or("?"),
                                r["severity_override"].as_str().unwrap_or("?"),
                                r["path_pattern"].as_str().unwrap_or("*"),
                                status,
                            );
                        }
                    }
                }
                RuleAction::Delete { id, api_url, token } => {
                    api_client::delete_evolution_rule(&api_url, &id, token.as_deref()).await?;
                    println!("Deleted rule {id}");
                }
                RuleAction::Toggle { id, enabled, api_url, token } => {
                    api_client::toggle_evolution_rule(&api_url, &id, enabled, token.as_deref()).await?;
                    println!("Rule {id} is now {}", if enabled { "enabled" } else { "disabled" });
                }
                RuleAction::Test { diff_id, api_url, token } => {
                    // Fetch the diff — evolution rules are already applied server-side.
                    let client = reqwest::Client::new();
                    let mut req = client.get(format!("{api_url}/v1/diffs/{diff_id}"));
                    if let Some(ref t) = token {
                        req = req.bearer_auth(t);
                    }
                    let resp = req.send().await?;
                    if !resp.status().is_success() {
                        anyhow::bail!("Failed to fetch diff {diff_id}: HTTP {}", resp.status());
                    }
                    let diff: serde_json::Value = resp.json().await?;
                    let changes = diff["changes"].as_array().cloned().unwrap_or_default();
                    let applied: Vec<_> = changes.iter()
                        .filter(|c| c.get("applied_rule").is_some())
                        .collect();
                    if applied.is_empty() {
                        println!("No evolution rules applied to diff {diff_id}.");
                    } else {
                        println!("Evolution rules applied to diff {}:\n", diff_id);
                        for c in &applied {
                            let rule = &c["applied_rule"];
                            println!(
                                "  {} {} — {} → {} (rule: {})",
                                c["kind"].as_str().unwrap_or("?"),
                                c["path"].as_str().unwrap_or("?"),
                                rule["original_severity"].as_str().unwrap_or("?"),
                                c["severity"].as_str().unwrap_or("?"),
                                rule["name"].as_str().unwrap_or("?"),
                            );
                        }
                    }
                }
            }
        }
        Commands::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "radar", &mut std::io::stdout());
        }
        Commands::GenerateTests {
            jira,
            jira_text,
            spec,
            base_url,
            out,
            postman_workspace,
        } => {
            // Resolve Jira content: API first, then --jira-text fallback.
            let (jira_summary, jira_description) = if let Some(ref key) = jira {
                let jira_base = std::env::var("JIRA_BASE_URL")
                    .map_err(|_| anyhow::anyhow!("JIRA_BASE_URL is not set"))?;
                let email = std::env::var("JIRA_EMAIL")
                    .map_err(|_| anyhow::anyhow!("JIRA_EMAIL is not set"))?;
                let token = std::env::var("JIRA_TOKEN")
                    .map_err(|_| anyhow::anyhow!("JIRA_TOKEN is not set"))?;
                let ticket = jira::fetch_ticket(&jira_base, &email, &token, key).await?;
                (ticket.summary, ticket.description)
            } else if let Some(text) = jira_text {
                let first_line = text.lines().next().unwrap_or("").to_string();
                (first_line, text)
            } else {
                return Err(anyhow::anyhow!(
                    "Provide either --jira PROJ-123 or --jira-text \"<ticket text>\""
                ));
            };

            // Resolve spec.
            let spec_yaml = match spec {
                Some(path) => std::fs::read_to_string(&path)
                    .map_err(|e| anyhow::anyhow!("cannot read spec '{}': {e}", path.display()))?,
                None => return Err(anyhow::anyhow!("--spec <path> is required")),
            };

            eprintln!("Generating tests for: {jira_summary}");
            let collection = test_gen::generate_test_collection(
                &jira_summary,
                &jira_description,
                &spec_yaml,
                &base_url,
            )
            .await?;

            let happy = collection.item.iter().filter(|i| i.name.starts_with("[HAPPY")).count();
            let negative = collection.item.len() - happy;
            eprintln!(
                "Generated {} test(s): {} happy-path, {} negative.",
                collection.item.len(),
                happy,
                negative
            );

            let json_str = serde_json::to_string_pretty(&collection)?;

            match out {
                Some(ref path) => {
                    std::fs::write(path, &json_str)?;
                    eprintln!("Collection written to {}", path.display());
                }
                None => println!("{json_str}"),
            }

            if let Some(ref workspace_id) = postman_workspace {
                let api_key = std::env::var("POSTMAN_API_KEY")
                    .map_err(|_| anyhow::anyhow!("POSTMAN_API_KEY is not set"))?;
                let url =
                    postman::push_collection(&api_key, Some(workspace_id), &collection).await?;
                eprintln!("Collection pushed to Postman: {url}");
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
            operation_map,
            collection,
        } => {
            // Parse --operation-map "field=METHOD /path" pairs into a lookup table.
            let op_map: std::collections::HashMap<String, String> = operation_map
                .iter()
                .filter_map(|s| {
                    let (field, op) = s.split_once('=')?;
                    Some((field.to_string(), op.to_string()))
                })
                .collect();

            if op_map.is_empty() {
                eprintln!("Note: no --operation-map provided — field-path-only matching will be used for blast-radius evidence.");
                eprintln!("      Use --operation-map \"field=METHOD /path\" to tie fields to concrete API operations.");
            }

            println!("Scanning {}…", source_dir.display());
            let records = radar_scanner::scan_directory(&source_dir);
            println!("Found {} property accesses.", records.len());

            if records.is_empty() {
                return Ok(());
            }

            let sites: Vec<api_client::CallSiteBody> = records
                .into_iter()
                .map(|r| {
                    // S2: use scanner-detected operation; fall back to --operation-map for S1
                    let operation = r
                        .operation
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| op_map.get(&r.field_path).cloned())
                        .unwrap_or_default();
                    api_client::CallSiteBody {
                        consumer_id: consumer_id.clone(),
                        service_id: service_id.clone(),
                        operation,
                        file_path: r.file_path,
                        line_number: r.line_number as i64,
                        field_path: r.field_path,
                    }
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

            // E-7: scan collection files and write impact_evidence directly.
            if !collection.is_empty() {
                println!("Scanning {} collection file(s)…", collection.len());
                for col_path in &collection {
                    match radar_scanner::parse_collection(col_path) {
                        Err(e) => eprintln!("Warning: skipping {}: {e}", col_path.display()),
                        Ok((col_name, requests)) => {
                            // Auto-register consumer by collection name
                            let resolved_consumer_id = match api_client::upsert_consumer_by_name(
                                &api_url, &col_name, "collection_file", token.as_deref(),
                            ).await {
                                Ok((id, created)) => {
                                    if created {
                                        println!("  Registered consumer '{col_name}' ({id})");
                                    }
                                    id
                                }
                                Err(e) => {
                                    eprintln!("Warning: could not register consumer '{col_name}': {e}; using --consumer-id");
                                    consumer_id.clone()
                                }
                            };

                            // Build evidence items — one per (request × field_path), or one
                            // with empty field_path when the request has no test assertions.
                            let mut evidence: Vec<api_client::CollectionEvidenceBody> = Vec::new();
                            let file_base = col_path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("<collection>");
                            for req in &requests {
                                let op = req.operation.as_deref().unwrap_or("").to_string();
                                if req.field_paths.is_empty() {
                                    evidence.push(api_client::CollectionEvidenceBody {
                                        consumer_id: resolved_consumer_id.clone(),
                                        service_id: service_id.clone(),
                                        operation: op,
                                        field_path: String::new(),
                                        evidence_uri: format!("file://{file_base}#{}", req.name),
                                    });
                                } else {
                                    for fp in &req.field_paths {
                                        evidence.push(api_client::CollectionEvidenceBody {
                                            consumer_id: resolved_consumer_id.clone(),
                                            service_id: service_id.clone(),
                                            operation: op.clone(),
                                            field_path: fp.clone(),
                                            evidence_uri: format!("file://{file_base}#{}", req.name),
                                        });
                                    }
                                }
                            }

                            match api_client::post_collection_evidence(&api_url, &evidence, token.as_deref()).await {
                                Ok((accepted, inserted)) => println!(
                                    "  {}: {accepted} request(s), {inserted} new evidence row(s)",
                                    col_path.display()
                                ),
                                Err(e) => eprintln!("Warning: failed to post collection evidence: {e}"),
                            }
                        }
                    }
                }
            }
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
                std::env::var("RADAR_SERVICE_TOKEN").ok().as_deref(),
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
