use serde_json::{json, Value};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Inline AI provider — mirrors radar-cli/src/ai_provider.rs
// (radar-api is a separate crate; duplication is intentional)
// ---------------------------------------------------------------------------

pub(crate) enum AiProvider {
    Anthropic { api_key: String },
    OpenAI { api_key: String, base_url: String },
    GitHubCopilot { token: String },
}

pub(crate) fn detect_provider() -> Option<AiProvider> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        if !k.is_empty() {
            return Some(AiProvider::Anthropic { api_key: k });
        }
    }
    if let Ok(k) = std::env::var("OPENAI_API_KEY") {
        if !k.is_empty() {
            let base = std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into());
            return Some(AiProvider::OpenAI { api_key: k, base_url: base });
        }
    }
    if let Ok(t) = std::env::var("GITHUB_COPILOT_TOKEN") {
        if !t.is_empty() {
            return Some(AiProvider::GitHubCopilot { token: t });
        }
    }
    None
}

impl AiProvider {
    pub(crate) async fn complete(&self, prompt: &str, max_tokens: u32) -> Option<String> {
        match self {
            Self::Anthropic { api_key } => {
                ai_call_anthropic(api_key, prompt, max_tokens).await
            }
            Self::OpenAI { api_key, base_url } => {
                ai_call_openai_compat(api_key, base_url, prompt, max_tokens).await
            }
            Self::GitHubCopilot { token } => {
                ai_call_openai_compat(token, "https://api.githubcopilot.com/v1", prompt, max_tokens).await
            }
        }
    }
}

async fn ai_call_anthropic(api_key: &str, prompt: &str, max_tokens: u32) -> Option<String> {
    let body = json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}]
    });
    let resp = reqwest::Client::new()
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
    let data: Value = resp.json().await.ok()?;
    data["content"].as_array()?
        .iter()
        .find(|b| b["type"] == "text")
        .and_then(|b| b["text"].as_str())
        .map(str::to_owned)
}

async fn ai_call_openai_compat(api_key: &str, base_url: &str, prompt: &str, max_tokens: u32) -> Option<String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = json!({
        "model": "gpt-4o",
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}]
    });
    let resp = reqwest::Client::new()
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
    let data: Value = resp.json().await.ok()?;
    data["choices"].as_array()?.first()
        .and_then(|c| c["message"]["content"].as_str())
        .map(str::to_owned)
}

pub(crate) fn build_both_formats(suite: Value, base_url: &str) -> (Value, String) {
    let apitesting_yaml = assemble_apitesting_yaml(&suite, base_url);
    let postman = assemble_postman_collection(suite, base_url);
    (postman, apitesting_yaml)
}

pub(crate) fn assemble_postman_collection(suite: Value, base_url: &str) -> Value {
    let collection_name = suite["collection_name"].as_str().unwrap_or("Generated Tests").to_string();
    let empty = vec![];
    let test_cases = suite["test_cases"].as_array().unwrap_or(&empty);

    let items: Vec<Value> = test_cases.iter().map(|tc| {
        let category = tc["category"].as_str().unwrap_or("test");
        let name = tc["name"].as_str().unwrap_or("Test");
        let method = tc["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = tc["path"].as_str().unwrap_or("/");
        let expected_status = tc["expected_status"].as_u64().unwrap_or(200);

        let path_segs: Vec<Value> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty())
            .map(|s| Value::String(s.to_string())).collect();

        let mut assertions = vec![
            format!("pm.test('Status is {expected_status}', () => pm.response.to.have.status({expected_status}));"),
        ];
        if let Some(arr) = tc["assertions"].as_array() {
            for a in arr {
                if let Some(s) = a.as_str() {
                    if !s.contains("have.status") {
                        assertions.push(s.to_string());
                    }
                }
            }
        }

        let has_body = !tc["body"].is_null() && tc["body"].is_object();
        let mut headers = vec![
            json!({"key": "Authorization", "value": "Bearer {{authToken}}", "type": "text"}),
        ];
        if has_body {
            headers.push(json!({"key": "Content-Type", "value": "application/json", "type": "text"}));
        }

        let body_json = if has_body {
            json!({
                "mode": "raw",
                "raw": serde_json::to_string_pretty(&tc["body"]).unwrap_or_default(),
                "options": {"raw": {"language": "json"}}
            })
        } else {
            Value::Null
        };

        let label = format!("[{}] {}", category.replace('_', " ").to_uppercase(), name);

        let mut item = json!({
            "name": label,
            "event": [{
                "listen": "test",
                "script": {
                    "type": "text/javascript",
                    "exec": assertions
                }
            }],
            "request": {
                "method": method,
                "header": headers,
                "url": {
                    "raw": format!("{{{{baseUrl}}}}{path}"),
                    "host": ["{{baseUrl}}"],
                    "path": path_segs,
                    "query": []
                }
            }
        });

        if !body_json.is_null() {
            item["request"]["body"] = body_json;
        }

        item
    }).collect();

    json!({
        "info": {
            "name": collection_name,
            "_postman_id": Uuid::new_v4().to_string(),
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": items,
        "variable": [
            {"key": "baseUrl", "value": base_url, "type": "string"},
            {"key": "authToken", "value": "", "type": "string"}
        ]
    })
}

/// Build an api-testing YAML suite from the raw Claude JSON value.
/// Internalises format patterns from https://github.com/LinuxSuRen/api-testing:
/// - `#!api-testing` magic header for auto-detection
/// - `param:` block with authToken for `{{.param.authToken}}` templating
/// - `expect.verify:` using the expr library (`data.field != null`)
/// - `expect.bodyFieldsExpect:` for simple field=value pins on happy-path tests
pub(crate) fn assemble_apitesting_yaml(suite: &Value, base_url: &str) -> String {
    #[derive(serde::Serialize)]
    struct Suite<'a> {
        name: &'a str,
        api: &'a str,
        param: std::collections::BTreeMap<&'static str, &'static str>,
        spec: Spec,
        items: Vec<TestCase>,
    }
    #[derive(serde::Serialize)]
    struct Spec { kind: &'static str }
    #[derive(serde::Serialize)]
    struct TestCase {
        name: String,
        request: TestRequest,
        expect: Expect,
    }
    #[derive(serde::Serialize)]
    struct TestRequest {
        api: String,
        method: String,
        header: std::collections::BTreeMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    }
    #[derive(serde::Serialize)]
    struct Expect {
        #[serde(rename = "statusCode")]
        status_code: u64,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        verify: Vec<String>,
        #[serde(rename = "bodyFieldsExpect", skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        body_fields_expect: std::collections::BTreeMap<String, Value>,
    }

    let collection_name = suite["collection_name"].as_str().unwrap_or("Generated Tests");
    let empty = vec![];
    let test_cases = suite["test_cases"].as_array().unwrap_or(&empty);

    let mut param = std::collections::BTreeMap::new();
    param.insert("authToken", "");

    let items: Vec<TestCase> = test_cases.iter().map(|tc| {
        let category = tc["category"].as_str().unwrap_or("test");
        let name = tc["name"].as_str().unwrap_or("Test");
        let method = tc["method"].as_str().unwrap_or("GET").to_uppercase();
        let path = tc["path"].as_str().unwrap_or("/").to_string();
        let status = tc["expected_status"].as_u64().unwrap_or(200);
        let has_body = tc["body"].is_object() && !tc["body"].as_object().map(|m| m.is_empty()).unwrap_or(true);

        let mut header = std::collections::BTreeMap::new();
        header.insert("Authorization".into(), "Bearer {{.param.authToken}}".into());
        if has_body {
            header.insert("Content-Type".into(), "application/json".into());
        }

        let body = if has_body {
            Some(serde_json::to_string_pretty(&tc["body"]).unwrap_or_default())
        } else {
            None
        };

        // Convert Postman assertions to api-testing expr verify expressions.
        let mut verify: Vec<String> = tc["assertions"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(|a| postman_assertion_to_verify(a.as_str().unwrap_or("")))
            .collect();

        if verify.is_empty() {
            verify.push(if category == "happy_path" {
                "data != null".into()
            } else {
                "data.error != null".into()
            });
        }

        // bodyFieldsExpect: pin top-level scalar fields from the request body for
        // happy-path tests as a lightweight contract check.
        let body_fields_expect = if category == "happy_path" {
            tc["body"].as_object()
                .map(|m| m.iter()
                    .filter(|(_, v)| v.is_string() || v.is_number() || v.is_boolean())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect())
                .unwrap_or_default()
        } else {
            std::collections::BTreeMap::new()
        };

        let label = format!("[{}] {name}", category.replace('_', " ").to_uppercase());
        TestCase {
            name: label,
            request: TestRequest { api: path, method, header, body },
            expect: Expect { status_code: status, verify, body_fields_expect },
        }
    }).collect();

    let s = Suite { name: collection_name, api: base_url, param, spec: Spec { kind: "openapi" }, items };
    match serde_yml::to_string(&s) {
        Ok(yaml) => format!("#!api-testing\n{yaml}"),
        Err(_) => String::from("#!api-testing\n# (yaml serialisation failed)\n"),
    }
}

/// Convert a Postman pm.test() assertion line to an api-testing expr verify expression.
pub(crate) fn postman_assertion_to_verify(assertion: &str) -> Option<String> {
    if assertion.contains("have.status") { return None; }
    if assertion.contains("headers.get") || assertion.contains("response.headers") { return None; }

    for q in ["'", "\""] {
        let pat = format!(".have.property({q}");
        if let Some(pos) = assertion.find(&pat) {
            let rest = &assertion[pos + pat.len()..];
            if let Some(end) = rest.find(q) {
                let field = &rest[..end];
                if !field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return Some(format!("data.{field} != null"));
                }
            }
        }
    }
    if assertion.contains(".to.have.length.above(0)") || assertion.contains("lengthOf.above(0)") {
        return Some("len(data) > 0".into());
    }
    None
}
