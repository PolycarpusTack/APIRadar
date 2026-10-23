use anyhow::Result;

pub struct JiraTicket {
    pub summary: String,
    pub description: String,
}

/// Fetch a Jira Cloud ticket via the REST API v2 (description returned as plain text).
/// Requires JIRA_BASE_URL, JIRA_EMAIL, JIRA_TOKEN environment variables.
pub async fn fetch_ticket(
    base_url: &str,
    email: &str,
    token: &str,
    key: &str,
) -> Result<JiraTicket> {
    let url = format!(
        "{}/rest/api/2/issue/{}",
        base_url.trim_end_matches('/'),
        key
    );

    // N-15: bound every request so a hung endpoint cannot stall the CLI.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    let resp = client
        .get(&url)
        .basic_auth(email, Some(token))
        .send()
        .await?
        .error_for_status()?;

    let body: serde_json::Value = resp.json().await?;
    let fields = &body["fields"];

    let description = fields["description"].as_str().unwrap_or("").to_string();

    Ok(JiraTicket {
        summary: fields["summary"].as_str().unwrap_or("").to_string(),
        description,
    })
}
