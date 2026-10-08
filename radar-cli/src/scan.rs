//! `radar scan` — post static call-site Evidence and collection-file Evidence.
//!
//! Extracted from `main.rs` (O-5) so the flow is integration-testable against
//! a mock API.

use crate::api_client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Run the scan flow: tree-sitter code scan → `POST /v1/call-sites`, then each
/// `--collection` file → consumer upsert + `POST /v1/evidence/collection`.
#[allow(clippy::too_many_arguments)] // mirrors the CLI surface
pub async fn run_scan(
    api_url: &str,
    consumer_id: &str,
    service_id: &str,
    source_dir: &Path,
    token: Option<&str>,
    op_map: &HashMap<String, String>,
    collections: &[PathBuf],
    json: bool,
) -> anyhow::Result<()> {
    // O-22: under --json, stdout carries one machine-readable summary object;
    // progress chatter goes to stderr so consumers can parse stdout directly.
    let mut posted_call_sites = 0usize;
    let mut collection_files = 0usize;
    let mut collection_evidence = 0usize;
    if op_map.is_empty() {
        eprintln!("Note: no --operation-map provided — field-path-only matching will be used for blast-radius evidence.");
        eprintln!("      Use --operation-map \"field=METHOD /path\" to tie fields to concrete API operations.");
    }

    if !json {
        println!("Scanning {}…", source_dir.display());
    }
    let records = radar_scanner::scan_directory(source_dir);
    if !json {
        println!("Found {} property accesses.", records.len());
    }

    // O-5: zero code call sites must NOT abort the run — a collections-only
    // Consumer repo still has `--collection` Evidence to post below. The old
    // early return here silently exited 0 and posted nothing.
    if records.is_empty() {
        if collections.is_empty() {
            if json {
                println!("{}", scan_summary(0, 0, 0));
            } else {
                println!(
                    "Nothing to post: no property accesses found and no --collection files given."
                );
            }
            return Ok(());
        }
    } else {
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
                    consumer_id: consumer_id.to_string(),
                    service_id: service_id.to_string(),
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
            match api_client::post_call_sites(api_url, chunk, token).await {
                Ok(n) => total += n,
                Err(e) => eprintln!("Warning: failed to post call sites: {e}"),
            }
        }
        posted_call_sites = total;
        if !json {
            println!("Posted {total} call site record(s) to {api_url}.");
        }
    }

    // E-7: scan collection files and write impact_evidence directly.
    if !collections.is_empty() {
        if !json {
            println!("Scanning {} collection file(s)…", collections.len());
        }
        for col_path in collections {
            match radar_scanner::parse_collection(col_path) {
                Err(e) => eprintln!("Warning: skipping {}: {e}", col_path.display()),
                Ok((col_name, requests)) => {
                    // Auto-register consumer by collection name
                    let resolved_consumer_id = match api_client::upsert_consumer_by_name(
                        api_url,
                        &col_name,
                        "collection_file",
                        token,
                    )
                    .await
                    {
                        Ok((id, created)) => {
                            if created && !json {
                                println!("  Registered consumer '{col_name}' ({id})");
                            }
                            id
                        }
                        Err(e) => {
                            eprintln!("Warning: could not register consumer '{col_name}': {e}; using --consumer-id");
                            consumer_id.to_string()
                        }
                    };

                    // Build evidence items — one per (request × field_path), or one
                    // with empty field_path when the request has no test assertions.
                    let mut evidence: Vec<api_client::CollectionEvidenceBody> = Vec::new();
                    let file_base = col_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("<collection>");
                    for req in &requests {
                        let op = req.operation.as_deref().unwrap_or("").to_string();
                        if req.field_paths.is_empty() {
                            evidence.push(api_client::CollectionEvidenceBody {
                                consumer_id: resolved_consumer_id.clone(),
                                service_id: service_id.to_string(),
                                operation: op,
                                field_path: String::new(),
                                evidence_uri: format!("file://{file_base}#{}", req.name),
                            });
                        } else {
                            for fp in &req.field_paths {
                                evidence.push(api_client::CollectionEvidenceBody {
                                    consumer_id: resolved_consumer_id.clone(),
                                    service_id: service_id.to_string(),
                                    operation: op.clone(),
                                    field_path: fp.clone(),
                                    evidence_uri: format!("file://{file_base}#{}", req.name),
                                });
                            }
                        }
                    }

                    match api_client::post_collection_evidence(api_url, &evidence, token).await {
                        Ok((accepted, inserted)) => {
                            collection_files += 1;
                            collection_evidence += inserted;
                            if !json {
                                println!(
                                    "  {}: {accepted} request(s), {inserted} new evidence row(s)",
                                    col_path.display()
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: failed to post collection evidence: {e}")
                        }
                    }
                }
            }
        }
    }
    if json {
        println!(
            "{}",
            scan_summary(posted_call_sites, collection_files, collection_evidence)
        );
    }
    Ok(())
}

/// One-line machine-readable summary of a scan run (O-22).
fn scan_summary(call_sites: usize, collection_files: usize, evidence_rows: usize) -> String {
    serde_json::json!({
        "posted_call_sites": call_sites,
        "collection_files": collection_files,
        "collection_evidence_rows": evidence_rows,
    })
    .to_string()
}
