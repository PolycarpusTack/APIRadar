/// Unified AI provider abstraction for Radar narrative and test generation.
///
/// Provider priority (first configured wins):
///   1. Unified AI Gateway — RADAR_AI_GATEWAY_URL (+ optional RADAR_AI_GATEWAY_KEY)
///      The suite-mandated path (O-19): one OpenAI-compatible endpoint that
///      routes to the optimal provider/model. Auth header is `X-API-Key`;
///      `model` is optional (the gateway auto-routes; RADAR_AI_MODEL is sent
///      as a hint when set).
///   2. Anthropic  — ANTHROPIC_API_KEY (direct; standalone deployments)
///   3. OpenAI     — OPENAI_API_KEY (+ optional OPENAI_BASE_URL for enterprise)
///
/// The GitHub Copilot path was removed (O-19 ST0): it posted a raw token to an
/// endpoint shape that never worked and failed silently.
///
/// All providers expose the same `complete(prompt, max_tokens) → Option<String>` surface.
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Provider enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Gateway {
        base_url: String,
        api_key: Option<String>,
        model: Option<String>,
    },
    Anthropic {
        api_key: String,
        model: String,
    },
    OpenAI {
        api_key: String,
        base_url: String,
        model: String,
    },
}

/// Default model for the direct-Anthropic path when RADAR_AI_MODEL is unset.
const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-6";
/// Default model for the direct-OpenAI path when RADAR_AI_MODEL is unset.
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";

impl Provider {
    /// Detect the first fully-configured provider from environment variables.
    pub fn detect() -> Option<Self> {
        Self::detect_from(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
    }

    /// Detection logic with an injectable environment, so priority rules are
    /// testable without process-global env-var races.
    pub fn detect_from(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let model_override = env("RADAR_AI_MODEL");
        if let Some(url) = env("RADAR_AI_GATEWAY_URL") {
            return Some(Self::Gateway {
                base_url: url,
                api_key: env("RADAR_AI_GATEWAY_KEY"),
                model: model_override,
            });
        }
        if let Some(k) = env("ANTHROPIC_API_KEY") {
            return Some(Self::Anthropic {
                api_key: k,
                model: model_override.unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.to_string()),
            });
        }
        if let Some(k) = env("OPENAI_API_KEY") {
            let base = env("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com/v1".into());
            return Some(Self::OpenAI {
                api_key: k,
                base_url: base,
                model: model_override.unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string()),
            });
        }
        None
    }

    /// Send `prompt` and return the response text, or None on failure.
    pub async fn complete(&self, prompt: &str, max_tokens: u32) -> Option<String> {
        match self {
            Self::Gateway {
                base_url,
                api_key,
                model,
            } => {
                call_gateway(
                    base_url,
                    api_key.as_deref(),
                    model.as_deref(),
                    prompt,
                    max_tokens,
                )
                .await
            }
            Self::Anthropic { api_key, model } => {
                call_anthropic(api_key, model, prompt, max_tokens).await
            }
            Self::OpenAI {
                api_key,
                base_url,
                model,
            } => call_openai_compat(api_key, base_url, model, prompt, max_tokens).await,
        }
    }
}

/// Convenience wrapper — detects provider then calls it.
/// Returns None if no provider is configured or the call fails.
pub async fn complete(prompt: &str, max_tokens: u32) -> Option<String> {
    Provider::detect()?.complete(prompt, max_tokens).await
}

// ---------------------------------------------------------------------------
// Unified AI Gateway (OpenAI-compatible; X-API-Key auth; model optional)
// ---------------------------------------------------------------------------

async fn call_gateway(
    base_url: &str,
    api_key: Option<&str>,
    model: Option<&str>,
    prompt: &str,
    max_tokens: u32,
) -> Option<String> {
    #[derive(Serialize)]
    struct Req<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<&'a str>,
        max_tokens: u32,
        messages: Vec<Msg<'a>>,
    }
    #[derive(Serialize)]
    struct Msg<'a> {
        role: &'static str,
        content: &'a str,
    }

    let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
    let body = Req {
        model,
        max_tokens,
        messages: vec![Msg {
            role: "user",
            content: prompt,
        }],
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_default();

    let mut req = client.post(&url).header("content-type", "application/json");
    if let Some(key) = api_key {
        req = req.header("X-API-Key", key);
    }
    let resp = req.json(&body).send().await.ok()?;

    if !resp.status().is_success() {
        tracing::warn!("AI Gateway error {}: {}", url, resp.status());
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    data["choices"]
        .as_array()?
        .first()
        .and_then(|c| c["message"]["content"].as_str())
        .map(str::to_owned)
}

// ---------------------------------------------------------------------------
// Anthropic Messages API (direct; standalone deployments without the gateway)
// ---------------------------------------------------------------------------

async fn call_anthropic(
    api_key: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> Option<String> {
    #[derive(Serialize)]
    struct Req<'a> {
        model: &'a str,
        max_tokens: u32,
        messages: Vec<Msg<'a>>,
    }
    #[derive(Serialize)]
    struct Msg<'a> {
        role: &'static str,
        content: &'a str,
    }

    let body = Req {
        model,
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
// (OpenAI, ChatGPT Enterprise via custom base URL)
// ---------------------------------------------------------------------------

async fn call_openai_compat(
    api_key: &str,
    base_url: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> Option<String> {
    #[derive(Serialize)]
    struct Req<'a> {
        model: &'a str,
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
        model,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| v.to_string())
        }
    }

    // O-19: the gateway is the suite-mandated path and wins over direct keys.
    #[test]
    fn gateway_outranks_direct_providers() {
        let p = Provider::detect_from(env_of(&[
            ("RADAR_AI_GATEWAY_URL", "http://localhost:8800"),
            ("ANTHROPIC_API_KEY", "sk-ant-x"),
            ("OPENAI_API_KEY", "sk-x"),
        ]))
        .expect("provider");
        assert!(
            matches!(p, Provider::Gateway { ref base_url, .. } if base_url == "http://localhost:8800"),
            "gateway must win: {p:?}"
        );
    }

    #[test]
    fn anthropic_used_without_gateway_and_model_overridable() {
        let p = Provider::detect_from(env_of(&[
            ("ANTHROPIC_API_KEY", "sk-ant-x"),
            ("RADAR_AI_MODEL", "claude-opus-5"),
        ]))
        .expect("provider");
        assert_eq!(
            p,
            Provider::Anthropic {
                api_key: "sk-ant-x".into(),
                model: "claude-opus-5".into()
            }
        );
    }

    #[test]
    fn anthropic_default_model_when_no_override() {
        let p = Provider::detect_from(env_of(&[("ANTHROPIC_API_KEY", "sk-ant-x")])).expect("p");
        assert!(
            matches!(p, Provider::Anthropic { ref model, .. } if model == DEFAULT_ANTHROPIC_MODEL)
        );
    }

    #[test]
    fn copilot_token_alone_configures_nothing() {
        // The Copilot path posted a raw token to an endpoint shape that never
        // worked, failing silently — deleted rather than kept broken (ST0).
        let p = Provider::detect_from(env_of(&[("GITHUB_COPILOT_TOKEN", "ghu_x")]));
        assert!(p.is_none(), "removed provider must not be detected: {p:?}");
    }

    #[test]
    fn no_config_yields_none() {
        assert!(Provider::detect_from(env_of(&[])).is_none());
    }
}
