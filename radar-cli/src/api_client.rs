use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Serialize;

use radar_core::diff::DiffChange;
use radar_core::models::Severity;

use crate::render::BlastRadiusResponse;

#[derive(Serialize)]
pub struct CallSiteBody {
    pub consumer_id: String,
    pub service_id: String,
    pub operation: String,
    pub file_path: String,
    pub line_number: i64,
    pub field_path: String,
}

// ---------------------------------------------------------------------------
// Request body types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PostDiffBody<'a> {
    service_name: &'a str,
    repo_url: &'a str,
    owner_team: &'a str,
    from_git_ref: &'a str,
    to_git_ref: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pr_url: Option<&'a str>,
    spec_format: &'a str,
    changes: Vec<ChangeBody<'a>>,
}

#[derive(Serialize)]
struct ChangeBody<'a> {
    path: &'a str,
    kind: &'a str,
    severity: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

// ---------------------------------------------------------------------------
// Parameters struct (avoids too-many-arguments lint)
// ---------------------------------------------------------------------------

/// Parameters for posting a diff to radar-api.
pub struct PostDiffParams<'a> {
    pub service_id: &'a str,
    pub service_name: &'a str,
    pub from_ref: &'a str,
    pub to_ref: &'a str,
    pub pr_url: Option<&'a str>,
    pub spec_format: &'a str,
    pub changes: &'a [DiffChange],
    pub token: Option<&'a str>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn severity_str(sev: &Severity) -> &'static str {
    match sev {
        Severity::Breaking => "breaking",
        Severity::NonBreakingRisky => "non_breaking_risky",
        Severity::Safe => "safe",
    }
}

fn build_client(token: Option<&str>) -> Result<(Client, reqwest::header::HeaderMap)> {
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

    let mut headers = HeaderMap::new();
    if let Some(t) = token {
        let bearer = format!("Bearer {t}");
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer).context("invalid token value")?,
        );
    }

    let client = Client::builder()
        .build()
        .context("failed to build HTTP client")?;

    Ok((client, headers))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// POST /v1/services/{service_id}/diffs — returns the new diff_id.
pub async fn post_diff(api_url: &str, p: PostDiffParams<'_>) -> Result<String> {
    let (client, headers) = build_client(p.token)?;

    let change_bodies: Vec<ChangeBody> = p
        .changes
        .iter()
        .map(|c| ChangeBody {
            path: &c.path,
            kind: c.kind.as_str(),
            severity: severity_str(&c.severity),
            description: c.description.as_deref(),
        })
        .collect();

    let body = PostDiffBody {
        service_name: p.service_name,
        repo_url: "",
        owner_team: "",
        from_git_ref: p.from_ref,
        to_git_ref: p.to_ref,
        pr_url: p.pr_url,
        spec_format: p.spec_format,
        changes: change_bodies,
    };

    let url = format!("{api_url}/v1/services/{}/diffs", p.service_id);
    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .context("failed to POST diff")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error posting diff: {} — {}", status, text);
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse POST diff response")?;
    let diff_id = json["id"]
        .as_str()
        .context("response missing 'id' field")?
        .to_string();

    Ok(diff_id)
}

/// POST /v1/call-sites — upsert a batch of call site records.
pub async fn post_call_sites(
    api_url: &str,
    sites: &[CallSiteBody],
    token: Option<&str>,
) -> Result<usize> {
    let (client, headers) = build_client(token)?;

    let url = format!("{api_url}/v1/call-sites");
    let resp = client
        .post(&url)
        .headers(headers)
        .json(sites)
        .send()
        .await
        .context("failed to POST call-sites")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error posting call-sites: {} — {}", status, text);
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse POST call-sites response")?;
    let accepted = json["accepted"].as_u64().unwrap_or(0) as usize;
    Ok(accepted)
}

/// POST /v1/policy-decisions — persist a policy verdict from a drift check run.
pub async fn post_policy_decision(
    api_url: &str,
    diff_id: Option<&str>,
    service_id: Option<&str>,
    verdict: &str,
    fail_mode: &str,
    actor: &str,
    token: Option<&str>,
) -> Result<String> {
    let (client, headers) = build_client(token)?;

    let body = serde_json::json!({
        "diff_id":    diff_id,
        "service_id": service_id,
        "verdict":    verdict,
        "fail_mode":  fail_mode,
        "actor":      actor,
    });

    let url = format!("{api_url}/v1/policy-decisions");
    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .context("failed to POST policy-decision")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error posting policy-decision: {} — {}", status, text);
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse POST policy-decision response")?;
    let id = json["id"]
        .as_str()
        .context("response missing 'id' field")?
        .to_string();

    Ok(id)
}

/// POST /v1/consumers/upsert — auto-register a consumer by name (idempotent on org+name).
/// Returns `(consumer_id, created)`.
pub async fn upsert_consumer_by_name(
    api_url: &str,
    name: &str,
    catalog_source: &str,
    token: Option<&str>,
) -> Result<(String, bool)> {
    let (client, headers) = build_client(token)?;

    let body = serde_json::json!({
        "name": name,
        "catalog_source": catalog_source,
    });

    let url = format!("{api_url}/v1/consumers/upsert");
    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .context("failed to POST consumers/upsert")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error upserting consumer: {} — {}", status, text);
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse upsert consumer response")?;
    let id = json["id"]
        .as_str()
        .context("response missing 'id' field")?
        .to_string();
    let created = json["created"].as_bool().unwrap_or(false);

    Ok((id, created))
}

/// Evidence item for a single collection request.
#[derive(serde::Serialize)]
pub struct CollectionEvidenceBody {
    pub consumer_id: String,
    pub service_id: String,
    pub operation: String,
    pub field_path: String,
    pub evidence_uri: String,
}

/// POST /v1/evidence/collection — write impact_evidence rows from a collection file scan.
/// Returns `(accepted, inserted)`.
pub async fn post_collection_evidence(
    api_url: &str,
    items: &[CollectionEvidenceBody],
    token: Option<&str>,
) -> Result<(usize, usize)> {
    let (client, headers) = build_client(token)?;

    let url = format!("{api_url}/v1/evidence/collection");
    let resp = client
        .post(&url)
        .headers(headers)
        .json(items)
        .send()
        .await
        .context("failed to POST evidence/collection")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error posting collection evidence: {} — {}", status, text);
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse collection evidence response")?;
    let accepted = json["accepted"].as_u64().unwrap_or(0) as usize;
    let inserted = json["inserted"].as_u64().unwrap_or(0) as usize;

    Ok((accepted, inserted))
}

/// GET /v1/diffs/{diff_id}/acknowledgements — returns true if at least one active ack exists.
/// Used by the CLI to automatically set verdict=overridden when a producer has formally acked.
pub async fn check_diff_acknowledged(
    api_url: &str,
    diff_id: &str,
    token: Option<&str>,
) -> Result<bool> {
    let (client, headers) = build_client(token)?;

    let url = format!("{api_url}/v1/diffs/{diff_id}/acknowledgements");
    let resp = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .context("failed to GET acknowledgements")?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse acknowledgements response")?;
    let count = json["entries"].as_array().map(|a| a.len()).unwrap_or(0);
    Ok(count > 0)
}

/// GET /v1/diffs/{diff_id}/blast-radius — returns the parsed blast radius response.
pub async fn get_blast_radius(
    api_url: &str,
    diff_id: &str,
    token: Option<&str>,
) -> Result<BlastRadiusResponse> {
    let (client, headers) = build_client(token)?;

    let url = format!("{api_url}/v1/diffs/{diff_id}/blast-radius");
    let resp = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .context("failed to GET blast-radius")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error fetching blast radius: {} — {}", status, text);
    }

    let br: BlastRadiusResponse = resp
        .json()
        .await
        .context("failed to parse blast-radius response")?;

    Ok(br)
}

// ---------------------------------------------------------------------------
// Evolution rules
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CreateRuleBody {
    pub name: String,
    pub change_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_pattern: Option<String>,
    pub severity_override: String,
}

pub async fn create_evolution_rule(
    api_url: &str,
    body: &CreateRuleBody,
    token: Option<&str>,
) -> Result<serde_json::Value> {
    let (client, headers) = build_client(token)?;
    let resp = client
        .post(format!("{api_url}/v1/evolution-rules"))
        .headers(headers)
        .json(body)
        .send()
        .await
        .context("failed to POST evolution-rules")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error creating rule: {status} — {text}");
    }
    resp.json().await.context("failed to parse create rule response")
}

pub async fn list_evolution_rules(
    api_url: &str,
    token: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let (client, headers) = build_client(token)?;
    let resp = client
        .get(format!("{api_url}/v1/evolution-rules"))
        .headers(headers)
        .send()
        .await
        .context("failed to GET evolution-rules")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error listing rules: {status} — {text}");
    }
    let json: serde_json::Value = resp.json().await.context("failed to parse list rules")?;
    Ok(json["entries"].as_array().cloned().unwrap_or_default())
}

pub async fn delete_evolution_rule(
    api_url: &str,
    rule_id: &str,
    token: Option<&str>,
) -> Result<()> {
    let (client, headers) = build_client(token)?;
    let resp = client
        .delete(format!("{api_url}/v1/evolution-rules/{rule_id}"))
        .headers(headers)
        .send()
        .await
        .context("failed to DELETE evolution-rule")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error deleting rule: {status} — {text}");
    }
    Ok(())
}

pub async fn toggle_evolution_rule(
    api_url: &str,
    rule_id: &str,
    enabled: bool,
    token: Option<&str>,
) -> Result<()> {
    let (client, headers) = build_client(token)?;
    let resp = client
        .patch(format!("{api_url}/v1/evolution-rules/{rule_id}"))
        .headers(headers)
        .json(&serde_json::json!({ "enabled": enabled }))
        .send()
        .await
        .context("failed to PATCH evolution-rule")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_else(|_| "<unreadable>".into());
        bail!("API error toggling rule: {status} — {text}");
    }
    Ok(())
}

/// H-5: Fetch test suites generated for a specific diff (used in PR comment).
pub async fn fetch_diff_test_suites(
    api_url: &str,
    diff_id: &str,
    token: Option<&str>,
) -> Result<Vec<crate::github::TestSuiteSummary>> {
    let (client, headers) = build_client(token)?;
    let resp = client
        .get(format!("{api_url}/v1/diffs/{diff_id}/test-suites"))
        .headers(headers)
        .send()
        .await
        .context("failed to GET diff test suites")?;
    if !resp.status().is_success() {
        return Ok(vec![]);
    }
    Ok(resp.json().await.unwrap_or_default())
}
