/// Generates api-testing YAML suites (https://github.com/LinuxSuRen/api-testing)
/// from the same intermediate GeneratedTestCase structs used for Postman output.
///
/// Format patterns internalised from the api-testing repo:
/// - `#!api-testing` magic header for auto-detection
/// - `api:` base URL at suite level; each item's `request.api` is the path
/// - `param:` block for named variables referenced via `{{.param.key}}`
/// - `spec.kind: openapi` links the suite back to the spec type
/// - `expect.verify:` uses the expr library — comparisons like `data.id != null`
/// - `expect.statusCode:` is separate from verify (no duplication)
/// - `expect.bodyFieldsExpect:` for simple key=value field assertions
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

use crate::test_gen::GeneratedTestCase;

// ---------------------------------------------------------------------------
// Serde types — map 1:1 to the api-testing YAML schema
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct Suite {
    name: String,
    api: String,
    param: HashMap<String, String>,
    spec: SuiteSpec,
    items: Vec<TestCase>,
}

#[derive(Serialize)]
struct SuiteSpec {
    kind: String,
}

#[derive(Serialize)]
struct TestCase {
    name: String,
    request: Request,
    expect: Expect,
}

#[derive(Serialize)]
struct Request {
    api: String,
    method: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    header: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
}

#[derive(Serialize)]
struct Expect {
    #[serde(rename = "statusCode")]
    status_code: u16,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    verify: Vec<String>,
    #[serde(rename = "bodyFieldsExpect", skip_serializing_if = "HashMap::is_empty")]
    body_fields_expect: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Assemble an api-testing YAML string from generated test cases.
pub fn assemble_suite(
    collection_name: &str,
    test_cases: &[GeneratedTestCase],
    base_url: &str,
) -> Result<String> {
    let mut param = HashMap::new();
    param.insert("authToken".into(), String::new());

    let items = test_cases.iter().map(build_test_case).collect();

    let suite = Suite {
        name: collection_name.to_string(),
        api: base_url.to_string(),
        param,
        spec: SuiteSpec { kind: "openapi".into() },
        items,
    };

    let yaml = serde_yml::to_string(&suite)?;
    // Prepend the api-testing magic comment used for auto-detection.
    Ok(format!("#!api-testing\n{yaml}"))
}

// ---------------------------------------------------------------------------
// Per-test-case assembly
// ---------------------------------------------------------------------------

fn build_test_case(tc: &GeneratedTestCase) -> TestCase {
    // Resolve path parameters inline.
    let mut path = tc.path.clone();
    for (k, v) in &tc.path_params {
        path = path.replace(&format!("{{{k}}}"), v);
        path = path.replace(&format!(":{k}"), v);
    }
    // Append query string.
    if !tc.query_params.is_empty() {
        let qs: String = tc
            .query_params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        path = format!("{path}?{qs}");
    }

    // Build headers — auth always present; Content-Type when body is set.
    let mut header = HashMap::new();
    header.insert(
        "Authorization".into(),
        "Bearer {{.param.authToken}}".into(),
    );
    let body_str = tc.body.as_ref().map(|b| {
        header.insert("Content-Type".into(), "application/json".into());
        serde_json::to_string_pretty(b).unwrap_or_default()
    });

    // Convert Postman pm.test() assertions → api-testing expr verify expressions.
    // Also extract simple field-equality assertions for bodyFieldsExpect.
    let mut verify: Vec<String> = tc
        .assertions
        .iter()
        .filter_map(|a| postman_to_verify(a))
        .collect();

    // Extract simple bodyFieldsExpect from body structure for happy-path tests.
    let body_fields_expect = if tc.category == "happy_path" {
        extract_body_fields(&tc.body)
    } else {
        HashMap::new()
    };

    // Guarantee at least one verify rule so the suite is not vacuous.
    if verify.is_empty() {
        let rule = if tc.category == "happy_path" {
            "data != null".into()
        } else {
            "data.error != null".into()
        };
        verify.push(rule);
    }

    let label = tc.category.replace('_', " ").to_uppercase();
    TestCase {
        name: format!("[{label}] {}", tc.name),
        request: Request {
            api: path,
            method: tc.method.to_uppercase(),
            header,
            body: body_str,
        },
        expect: Expect {
            status_code: tc.expected_status,
            verify,
            body_fields_expect,
        },
    }
}

// ---------------------------------------------------------------------------
// Assertion conversion helpers
// ---------------------------------------------------------------------------

/// Convert a single Postman JavaScript assertion line to an api-testing
/// expr-library verify expression, or return None if no mapping exists.
///
/// Patterns handled:
/// `.have.property('field')` → `data.field != null`
/// `.be.above(N)`            → `data > N`
/// `.have.length.above(N)`   → `len(data) > N`
fn postman_to_verify(assertion: &str) -> Option<String> {
    // Skip status assertions — covered by statusCode field.
    if assertion.contains("have.status") || assertion.contains("to.have.status") {
        return None;
    }
    // Skip header assertions — api-testing handles these differently.
    if assertion.contains("headers.get") || assertion.contains("response.headers") {
        return None;
    }

    // .have.property('fieldName') or .have.property("fieldName")
    for q in ["'", "\""] {
        let pat = format!(".have.property({q}");
        if let Some(pos) = assertion.find(&pat) {
            let rest = &assertion[pos + pat.len()..];
            if let Some(end) = rest.find(q) {
                let field = &rest[..end];
                if is_safe_field(field) {
                    return Some(format!("data.{field} != null"));
                }
            }
        }
    }

    // .to.be.above(N) — numeric lower bound on the root value
    if let Some(pos) = assertion.find(".to.be.above(") {
        let rest = &assertion[pos + 13..];
        if let Some(end) = rest.find(')') {
            let n = rest[..end].trim();
            if n.parse::<f64>().is_ok() {
                return Some(format!("data > {n}"));
            }
        }
    }

    // len(data) > 0 — common list-endpoint assertion
    if assertion.contains(".to.have.length.above(0)")
        || assertion.contains(".lengthOf.above(0)")
        || assertion.contains(".length).to.be.above(0)")
    {
        return Some("len(data) > 0".into());
    }

    None
}

/// Returns true if the field name is safe to embed directly in an expr expression.
fn is_safe_field(field: &str) -> bool {
    !field.is_empty()
        && field
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// For happy-path tests, build bodyFieldsExpect entries from top-level body
/// fields that have simple scalar values — useful as additional contract pins.
fn extract_body_fields(
    body: &Option<serde_json::Value>,
) -> HashMap<String, serde_json::Value> {
    let mut out = HashMap::new();
    if let Some(serde_json::Value::Object(map)) = body {
        for (k, v) in map {
            if (v.is_string() || v.is_number() || v.is_boolean()) && is_safe_field(k) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}
