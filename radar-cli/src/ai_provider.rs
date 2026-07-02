/// Unified AI provider abstraction for Radar narrative and test generation.
///
/// Provider priority (first configured wins):
///   1. Anthropic  — ANTHROPIC_API_KEY
///   2. OpenAI     — OPENAI_API_KEY (+ optional OPENAI_BASE_URL for enterprise)
///   3. GitHub Copilot — GITHUB_COPILOT_TOKEN
///
/// All providers expose the same `complete(prompt, max_tokens) → Option<String>` surface.
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Provider enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Provider {
    Anthropic { api_key: String },
    OpenAI { api_key: String, base_url: String },
    GitHubCopilot { token: String },
}

impl Provider {
    /// Detect the first fully-configured provider from environment variables.
    pub fn detect() -> Option<Self> {
        if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
            if !k.is_empty() {
                return Some(Self::Anthropic { api_key: k });
            }
        }
        if let Ok(k) = std::env::var("OPENAI_API_KEY") {
            if !k.is_empty() {
                let base = std::env::var("OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".into());
                return Some(Self::OpenAI {
                    api_key: k,
                    base_url: base,
                });
            }
        }
        if let Ok(t) = std::env::var("GITHUB_COPILOT_TOKEN") {
            if !t.is_empty() {
                return Some(Self::GitHubCopilot { token: t });
            }
        }
        None
    }

    /// Send `prompt` and return the response text, or None on failure.
    pub async fn complete(&self, prompt: &str, max_tokens: u32) -> Option<String> {
        match self {
            Self::Anthropic { api_key } => call_anthropic(api_key, prompt, max_tokens).await,
            Self::OpenAI { api_key, base_url } => {
                call_openai_compat(api_key, base_url, prompt, max_tokens).await
            }
            Self::GitHubCopilot { token } => {
                call_openai_compat(
                    token,
                    "https://api.githubcopilot.com/v1",
                    prompt,
                    max_tokens,
                )
                .await
            }
        }
    }
}

/// Convenience wrapper — detects provider then calls it.
/// Returns None if no provider is configured or the call fails.
pub async fn complete(prompt: &str, max_tokens: u32) -> Option<String> {
    Provider::detect()?.complete(prompt, max_tokens).await
}

// ---------------------------------------------------------------------------
// Anthropic Messages API
// ---------------------------------------------------------------------------

async fn call_anthropic(api_key: &str, prompt: &str, max_tokens: u32) -> Option<String> {
    #[derive(Serialize)]
    struct Req<'a> {
        model: &'static str,
        max_tokens: u32,
        messages: Vec<Msg<'a>>,
    }
    #[derive(Serialize)]
    struct Msg<'a> {
        role: &'static str,
        content: &'a str,
    }

    let body = Req {
        model: "claude-sonnet-4-6",
        max_tokens,
        messages: vec![Msg {
            role: "user",
            content: prompt,
        }],
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!("Anthropic API error: {}", resp.status());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    data["content"]
        .as_array()?
        .iter()
        .find(|b| b["type"] == "text")
        .and_then(|b| b["text"].as_str())
        .map(str::to_owned)
}

// ---------------------------------------------------------------------------
// OpenAI-compatible Chat Completions API
// (OpenAI, ChatGPT Enterprise via custom base URL, GitHub Copilot)
// ---------------------------------------------------------------------------

async fn call_openai_compat(
    api_key: &str,
    base_url: &str,
    prompt: &str,
    max_tokens: u32,
) -> Option<String> {
    #[derive(Serialize)]
    struct Req<'a> {
        model: &'static str,
        max_tokens: u32,
        messages: Vec<Msg<'a>>,
    }
    #[derive(Serialize)]
    struct Msg<'a> {
        role: &'static str,
        content: &'a str,
    }

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = Req {
        model: "gpt-4o",
        max_tokens,
        messages: vec![Msg {
            role: "user",
            content: prompt,
        }],
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!("OpenAI-compat API error {}: {}", url, resp.status());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    data["choices"]
        .as_array()?
        .first()
        .and_then(|c| c["message"]["content"].as_str())
        .map(str::to_owned)
}
