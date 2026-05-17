use reqwest::Client;
use serde::{Deserialize, Serialize};

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-sonnet-4-6";

#[derive(Serialize)]
struct ClaudeRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ClaudeMessage<'a>>,
}

#[derive(Serialize)]
struct ClaudeMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

/// Generate a plain-language explanation of a set of breaking API changes.
/// Returns None if ANTHROPIC_API_KEY is not set (graceful degradation).
pub async fn generate_narrative(changes_summary: &str) -> Option<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;

    let prompt = format!(
        "You are a technical writer for an API platform team. \
         Write a clear, concise (2-3 sentences) plain-language explanation of these API breaking changes \
         for consumers who need to update their integrations. Be specific about impact.\n\n\
         Changes:\n{changes_summary}"
    );

    let client = Client::new();
    let body = ClaudeRequest {
        model: MODEL,
        max_tokens: 256,
        messages: vec![ClaudeMessage {
            role: "user",
            content: &prompt,
        }],
    };

    let resp = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let data: ClaudeResponse = resp.json().await.ok()?;
    data.content
        .into_iter()
        .find(|b| b.block_type == "text")
        .and_then(|b| b.text)
}
