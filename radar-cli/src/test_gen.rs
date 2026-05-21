use crate::postman::{
    Body, BodyOptions, Collection, Event, Header, Info, Item, PostmanRequest, QueryParam,
    RawOptions, Script, Url, Variable,
};
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AI response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GeneratedSuite {
    collection_name: String,
    test_cases: Vec<GeneratedTestCase>,
}

#[derive(Deserialize)]
pub struct GeneratedTestCase {
    pub name: String,
    pub category: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub path_params: HashMap<String, String>,
    #[serde(default)]
    pub query_params: HashMap<String, String>,
    pub body: Option<serde_json::Value>,
    pub expected_status: u16,
    #[serde(default)]
    pub assertions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate both a Postman Collection v2.1 and an api-testing YAML suite from a
/// single Claude call. Returns `(postman_collection, apitesting_yaml)`.
pub async fn generate_both(
    jira_summary: &str,
    jira_description: &str,
    spec_yaml: &str,
    base_url: &str,
) -> anyhow::Result<(Collection, String)> {
    let suite = call_claude(jira_summary, jira_description, spec_yaml).await?;
    let yaml = crate::apitesting::assemble_suite(&suite.collection_name, &suite.test_cases, base_url)
        .unwrap_or_default();
    let collection = assemble_collection(suite, base_url);
    Ok((collection, yaml))
}

/// Convenience wrapper that returns only the Postman Collection (discards YAML).
pub async fn generate_test_collection(
    jira_summary: &str,
    jira_description: &str,
    spec_yaml: &str,
    base_url: &str,
) -> anyhow::Result<Collection> {
    let (collection, _yaml) = generate_both(jira_summary, jira_description, spec_yaml, base_url).await?;
    Ok(collection)
}

/// Call the configured AI provider once and return the parsed intermediate suite.
async fn call_claude(
    jira_summary: &str,
    jira_description: &str,
    spec_yaml: &str,
) -> Result<GeneratedSuite> {
    let spec_excerpt = if spec_yaml.len() > 40_000 { &spec_yaml[..40_000] } else { spec_yaml };
    let prompt = build_prompt(jira_summary, jira_description, spec_excerpt);

    let raw_text = crate::ai_provider::complete(&prompt, 4096)
        .await
        .ok_or_else(|| anyhow::anyhow!("No AI provider configured (set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GITHUB_COPILOT_TOKEN)"))?;

    let json_str = extract_json(&raw_text)
        .ok_or_else(|| anyhow::anyhow!("AI response did not contain a JSON object"))?;

    serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("failed to parse AI response: {e}\n\nRaw:\n{json_str}"))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn build_prompt(summary: &str, description: &str, spec: &str) -> String {
    format!(
        r#"You are a QA engineer generating Postman API tests from a Jira ticket and an OpenAPI spec.

## Jira Ticket
Title: {summary}
Description:
{description}

## OpenAPI Specification
```yaml
{spec}
```

## Task
Generate API test cases covering:
1. Happy-path tests — valid inputs, successful operations that satisfy the ticket's acceptance criteria
2. Negative tests — missing required fields (→ 400/422), wrong types (→ 400), unauthorized (→ 401), not-found (→ 404), boundary values

Rules:
- Derive tests from both the Jira description and the OpenAPI spec
- Use {{{{baseUrl}}}} as the host placeholder and {{{{authToken}}}} for bearer auth
- Each assertion must be a complete, valid JavaScript pm.test() statement on a single line
- Aim for 4–6 happy-path tests and 4–8 negative tests
- Return ONLY valid JSON — no markdown fences, no explanation before or after

Required output format (JSON only):
{{
  "collection_name": "TICKET-KEY — Short Title",
  "test_cases": [
    {{
      "name": "Happy Path — create resource with all required fields",
      "category": "happy_path",
      "method": "POST",
      "path": "/v1/resource",
      "path_params": {{}},
      "query_params": {{}},
      "body": {{"field": "value"}},
      "expected_status": 201,
      "assertions": [
        "pm.test('Response has id', () => {{ pm.expect(pm.response.json()).to.have.property('id'); }});",
        "pm.test('Content-Type is JSON', () => {{ pm.expect(pm.response.headers.get('Content-Type')).to.include('application/json'); }});"
      ]
    }},
    {{
      "name": "Negative — missing required field name",
      "category": "negative",
      "method": "POST",
      "path": "/v1/resource",
      "path_params": {{}},
      "query_params": {{}},
      "body": {{}},
      "expected_status": 422,
      "assertions": [
        "pm.test('Error body is present', () => {{ pm.expect(pm.response.json()).to.have.property('error'); }});"
      ]
    }}
  ]
}}"#
    )
}

/// Extract the first complete JSON object from a string (handles surrounding prose).
fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start <= end {
        Some(&text[start..=end])
    } else {
        None
    }
}

fn assemble_collection(suite: GeneratedSuite, base_url: &str) -> Collection {
    let items = suite.test_cases.into_iter().map(|tc| {
        // Substitute path parameters.
        let mut resolved = tc.path.clone();
        for (k, v) in &tc.path_params {
            resolved = resolved.replace(&format!("{{{k}}}"), v);
            resolved = resolved.replace(&format!(":{k}"), v);
        }

        let path_segs: Vec<String> = resolved
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();

        let query: Vec<QueryParam> = tc
            .query_params
            .into_iter()
            .map(|(k, v)| QueryParam { key: k, value: v })
            .collect();

        let raw_url = if query.is_empty() {
            format!("{{{{baseUrl}}}}{resolved}")
        } else {
            let qs: String = query
                .iter()
                .map(|q| format!("{}={}", q.key, q.value))
                .collect::<Vec<_>>()
                .join("&");
            format!("{{{{baseUrl}}}}{resolved}?{qs}")
        };

        let body = tc.body.map(|b| Body {
            mode: "raw".into(),
            raw: serde_json::to_string_pretty(&b).unwrap_or_default(),
            options: BodyOptions {
                raw: RawOptions {
                    language: "json".into(),
                },
            },
        });

        let mut headers = vec![Header {
            key: "Authorization".into(),
            value: "Bearer {{authToken}}".into(),
            header_type: "text".into(),
        }];
        if body.is_some() {
            headers.push(Header {
                key: "Content-Type".into(),
                value: "application/json".into(),
                header_type: "text".into(),
            });
        }

        // Always include a status assertion; skip any Claude-generated duplicates.
        let mut exec = vec![format!(
            "pm.test('Status is {}', () => pm.response.to.have.status({}));",
            tc.expected_status, tc.expected_status
        )];
        for a in &tc.assertions {
            if !a.contains("have.status") {
                exec.push(a.clone());
            }
        }

        Item {
            name: format!("[{}] {}", tc.category.replace('_', " ").to_uppercase(), tc.name),
            event: vec![Event {
                listen: "test".into(),
                script: Script {
                    script_type: "text/javascript".into(),
                    exec,
                },
            }],
            request: PostmanRequest {
                method: tc.method.to_uppercase(),
                header: headers,
                body,
                url: Url {
                    raw: raw_url,
                    host: vec!["{{baseUrl}}".into()],
                    path: path_segs,
                    query,
                },
            },
        }
    });

    Collection {
        info: Info {
            name: suite.collection_name,
            _postman_id: uuid::Uuid::new_v4().to_string(),
            schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json".into(),
        },
        item: items.collect(),
        variable: vec![
            Variable {
                key: "baseUrl".into(),
                value: base_url.to_string(),
                var_type: "string".into(),
            },
            Variable {
                key: "authToken".into(),
                value: String::new(),
                var_type: "string".into(),
            },
        ],
    }
}
