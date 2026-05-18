use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Serialize;

use drift_core::diff::DiffChange;
use drift_core::models::{ChangeKind, Severity};

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

/// Parameters for posting a diff to drift-api.
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

fn kind_str(kind: &ChangeKind) -> &'static str {
    match kind {
        ChangeKind::FieldRemoved => "field_removed",
        ChangeKind::FieldAdded => "field_added",
        ChangeKind::TypeChanged => "type_changed",
        ChangeKind::RequiredChanged => "required_changed",
        ChangeKind::OperationRemoved => "operation_removed",
        ChangeKind::OperationAdded => "operation_added",
    }
}

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
            kind: kind_str(&c.kind),
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
