use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::{JwtClaims, assert_org_access};
use crate::errors::ApiError;
use crate::ai::{detect_provider, build_both_formats};
use crate::PaginationParams;

#[derive(serde::Deserialize)]
pub(crate) struct GenerateTestsBody {
    pub(crate) spec_yaml: Option<String>,
    pub(crate) diff_id: Option<String>,
    pub(crate) jira_key: Option<String>,
    pub(crate) jira_text: Option<String>,
    pub(crate) service_id: Option<String>,
    pub(crate) consumer_id: Option<String>,
    #[serde(default)]
    pub(crate) use_templates: bool,
    #[serde(default = "default_base_url")]
    pub(crate) base_url: String,
}

fn default_base_url() -> String {
    "http://localhost:8080".to_string()
}

// POST /v1/generate-tests
pub(crate) async fn generate_tests(
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<GenerateTestsBody>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let has_jira = body.jira_key.is_some() || body.jira_text.is_some();
    let has_diff = body.diff_id.is_some();

    if !has_jira && !has_diff && body.spec_yaml.is_none() {
        return Err(ApiError::BadRequest(
            "Provide jira_key, jira_text, or diff_id".to_string(),
        ));
    }

    if has_diff && !has_jira {
        let diff_id = body.diff_id.as_deref().unwrap();
        let changes_rows = sqlx::query(
            "SELECT path, kind, severity FROM change WHERE diff_id = ?",
        )
        .bind(diff_id)
        .fetch_all(&pool)
        .await?;

        if changes_rows.is_empty() {
            return Err(ApiError::NotFound(format!("diff {diff_id} has no changes")));
        }

        let changes: Vec<Value> = changes_rows.iter().map(|r| json!({
            "path":     r.try_get::<String, _>("path").unwrap_or_default(),
            "kind":     r.try_get::<String, _>("kind").unwrap_or_default(),
            "severity": r.try_get::<String, _>("severity").unwrap_or_default(),
        })).collect();

        let evidence = load_diff_evidence(&pool, diff_id, body.consumer_id.as_deref()).await?;

        let use_templates = body.use_templates || detect_provider().is_none();

        let suite_raw = if use_templates {
            templates_from_changes(&changes, &evidence)
        } else {
            let context = format_diff_test_context(&changes, &evidence);
            let spec_yaml = resolve_spec_yaml(&pool, Some(diff_id), body.spec_yaml.as_deref()).await?;
            call_ai_for_tests_from_diff(&context, &spec_yaml)
                .await
                .map_err(|e| ApiError::BadRequest(format!("test generation failed: {e}")))?
        };

        let (collection_json, apitesting_yaml) = build_both_formats(suite_raw, &body.base_url);
        let items = collection_json["item"].as_array();
        let test_count = items.map(|a| a.len()).unwrap_or(0) as i64;
        let happy_count = items
            .map(|a| a.iter().filter(|i| i["name"].as_str().unwrap_or("").starts_with("[HAPPY")).count())
            .unwrap_or(0) as i64;
        let negative_count = test_count - happy_count;
        let collection_name = collection_json["info"]["name"]
            .as_str()
            .unwrap_or("Contract Compliance Tests")
            .to_string();

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let collection_str = serde_json::to_string(&collection_json).unwrap_or_default();

        sqlx::query(
            r#"INSERT INTO generated_test_suite
               (id, service_id, diff_id, consumer_id, jira_key, jira_summary, collection_name,
                collection_json, test_count, happy_count, negative_count, created_at, apitesting_yaml)
               VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(&body.service_id)
        .bind(diff_id)
        .bind(&body.consumer_id)
        .bind(&collection_name)
        .bind(&collection_name)
        .bind(&collection_str)
        .bind(test_count)
        .bind(happy_count)
        .bind(negative_count)
        .bind(&now)
        .bind(&apitesting_yaml)
        .execute(&pool)
        .await?;

        metrics::counter!("radar_test_suites_created_total").increment(1);

        return Ok((
            StatusCode::CREATED,
            Json(json!({
                "id":               id,
                "diff_id":          diff_id,
                "collection_name":  collection_name,
                "test_count":       test_count,
                "happy_count":      happy_count,
                "negative_count":   negative_count,
                "collection_json":  collection_json,
                "apitesting_yaml":  apitesting_yaml,
                "created_at":       now,
            })),
        ));
    }

    let (jira_summary, jira_description) = match body.jira_key {
        Some(ref key) => {
            let result = fetch_jira_ticket(key).await;
            match result {
                Ok((s, d)) => (s, d),
                Err(e) => {
                    if let Some(text) = body.jira_text.clone() {
                        let first = text.lines().next().unwrap_or("").to_string();
                        (first, text)
                    } else {
                        return Err(ApiError::BadRequest(format!(
                            "Jira fetch failed and no jira_text provided: {e}"
                        )));
                    }
                }
            }
        }
        None => match body.jira_text.clone() {
            Some(text) => {
                let first = text.lines().next().unwrap_or("").to_string();
                (first, text)
            }
            None => {
                return Err(ApiError::BadRequest(
                    "Provide either jira_key or jira_text".to_string(),
                ))
            }
        },
    };

    let spec_yaml = resolve_spec_yaml(&pool, body.diff_id.as_deref(), body.spec_yaml.as_deref()).await?;

    let suite_raw =
        call_ai_for_tests(&jira_summary, &jira_description, &spec_yaml)
            .await
            .map_err(|e| ApiError::BadRequest(format!("test generation failed: {e}")))?;

    let (collection_json, apitesting_yaml) = build_both_formats(suite_raw, &body.base_url);

    let items = collection_json["item"].as_array();
    let test_count = items.map(|a| a.len()).unwrap_or(0) as i64;
    let happy_count = items
        .map(|a| a.iter().filter(|i| i["name"].as_str().unwrap_or("").starts_with("[HAPPY")).count())
        .unwrap_or(0) as i64;
    let negative_count = test_count - happy_count;
    let collection_name = collection_json["info"]["name"].as_str().unwrap_or("Generated Tests").to_string();

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let collection_str = serde_json::to_string(&collection_json).unwrap_or_default();

    sqlx::query(
        r#"INSERT INTO generated_test_suite
           (id, service_id, diff_id, consumer_id, jira_key, jira_summary, collection_name, collection_json,
            test_count, happy_count, negative_count, created_at, apitesting_yaml)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&body.service_id)
    .bind(&body.diff_id)
    .bind(&body.consumer_id)
    .bind(&body.jira_key)
    .bind(&jira_summary)
    .bind(&collection_name)
    .bind(&collection_str)
    .bind(test_count)
    .bind(happy_count)
    .bind(negative_count)
    .bind(&now)
    .bind(&apitesting_yaml)
    .execute(&pool)
    .await?;

    metrics::counter!("radar_test_suites_created_total").increment(1);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":               id,
            "collection_name":  collection_name,
            "test_count":       test_count,
            "happy_count":      happy_count,
            "negative_count":   negative_count,
            "collection_json":  collection_json,
            "apitesting_yaml":  apitesting_yaml,
            "created_at":       now,
        })),
    ))
}

// GET /v1/generate-tests
pub(crate) async fn list_test_suites(
    State(pool): State<sqlx::AnyPool>,
    Query(page): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let limit = page.limit.clamp(1, 200);
    let offset = page.offset.max(0);

    let rows = sqlx::query(
        r#"SELECT id, service_id, jira_key, jira_summary, collection_name,
                  test_count, happy_count, negative_count, created_at
           FROM generated_test_suite
           ORDER BY created_at DESC
           LIMIT ? OFFSET ?"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":              r.get::<String, _>("id"),
                "service_id":      r.try_get::<Option<String>, _>("service_id").unwrap_or(None),
                "jira_key":        r.try_get::<Option<String>, _>("jira_key").unwrap_or(None),
                "jira_summary":    r.try_get::<Option<String>, _>("jira_summary").unwrap_or(None),
                "collection_name": r.get::<String, _>("collection_name"),
                "test_count":      r.try_get::<i64, _>("test_count").unwrap_or(0),
                "happy_count":     r.try_get::<i64, _>("happy_count").unwrap_or(0),
                "negative_count":  r.try_get::<i64, _>("negative_count").unwrap_or(0),
                "created_at":      r.get::<String, _>("created_at"),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// GET /v1/diffs/:id/test-suites
pub(crate) async fn list_diff_test_suites(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    let rows = sqlx::query(
        r#"SELECT id, collection_name, test_count, happy_count, negative_count, consumer_id, created_at
           FROM generated_test_suite
           WHERE diff_id = ?
           ORDER BY created_at DESC"#,
    )
    .bind(&diff_id)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows.iter().map(|r| json!({
        "id":              r.get::<String, _>("id"),
        "collection_name": r.get::<String, _>("collection_name"),
        "test_count":      r.try_get::<i64, _>("test_count").unwrap_or(0),
        "happy_count":     r.try_get::<i64, _>("happy_count").unwrap_or(0),
        "negative_count":  r.try_get::<i64, _>("negative_count").unwrap_or(0),
        "consumer_id":     r.try_get::<Option<String>, _>("consumer_id").unwrap_or(None),
        "created_at":      r.get::<String, _>("created_at"),
    })).collect();

    Ok(Json(json!(items)))
}

// GET /v1/generate-tests/:id
pub(crate) async fn get_test_suite(
    Path(suite_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let caller_org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let row = sqlx::query(
        r#"SELECT ts.id, ts.service_id, ts.jira_key, ts.jira_summary, ts.collection_name,
                  ts.collection_json, ts.apitesting_yaml, ts.test_count, ts.happy_count,
                  ts.negative_count, ts.created_at, s.org_id AS service_org_id
           FROM generated_test_suite ts
           LEFT JOIN service s ON s.id = ts.service_id
           WHERE ts.id = ?"#,
    )
    .bind(&suite_id)
    .fetch_optional(&pool)
    .await?;

    match row {
        None => Err(ApiError::NotFound(format!("test suite {suite_id} not found"))),
        Some(r) => {
            let row_org_id: String = r.try_get("service_org_id").unwrap_or_default();
            assert_org_access(&row_org_id, &caller_org_id, &format!("test suite {suite_id}"))?;
            let collection_json: Value = r
                .try_get::<String, _>("collection_json")
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            let apitesting_yaml = r.try_get::<Option<String>, _>("apitesting_yaml").unwrap_or(None);
            Ok((
                StatusCode::OK,
                Json(json!({
                    "id":               r.get::<String, _>("id"),
                    "service_id":       r.try_get::<Option<String>, _>("service_id").unwrap_or(None),
                    "jira_key":         r.try_get::<Option<String>, _>("jira_key").unwrap_or(None),
                    "jira_summary":     r.try_get::<Option<String>, _>("jira_summary").unwrap_or(None),
                    "collection_name":  r.get::<String, _>("collection_name"),
                    "collection_json":  collection_json,
                    "apitesting_yaml":  apitesting_yaml,
                    "test_count":       r.try_get::<i64, _>("test_count").unwrap_or(0),
                    "happy_count":      r.try_get::<i64, _>("happy_count").unwrap_or(0),
                    "negative_count":   r.try_get::<i64, _>("negative_count").unwrap_or(0),
                    "created_at":       r.get::<String, _>("created_at"),
                })),
            ))
        }
    }
}

pub(crate) async fn load_diff_evidence(
    pool: &sqlx::AnyPool,
    diff_id: &str,
    consumer_id: Option<&str>,
) -> Result<Vec<Value>, ApiError> {
    use sqlx::Row;
    let mut q = String::from(
        "SELECT ie.consumer_id, c.name AS consumer_name, ie.source_type, ie.operation, ie.field_path, ie.confidence, ie.observed_at \
         FROM impact_evidence ie LEFT JOIN consumer c ON c.id = ie.consumer_id \
         WHERE ie.diff_id = ?"
    );
    if consumer_id.is_some() {
        q.push_str(" AND ie.consumer_id = ?");
    }
    q.push_str(" ORDER BY ie.observed_at DESC LIMIT 200");

    let mut qb = sqlx::query(&q).bind(diff_id);
    if let Some(cid) = consumer_id {
        qb = qb.bind(cid);
    }
    let rows = qb.fetch_all(pool).await?;
    Ok(rows.iter().map(|r| json!({
        "consumer_id":   r.try_get::<String, _>("consumer_id").unwrap_or_default(),
        "consumer_name": r.try_get::<Option<String>, _>("consumer_name").unwrap_or(None),
        "source_type":   r.try_get::<String, _>("source_type").unwrap_or_default(),
        "operation":     r.try_get::<Option<String>, _>("operation").unwrap_or(None),
        "field_path":    r.try_get::<Option<String>, _>("field_path").unwrap_or(None),
        "confidence":    r.try_get::<String, _>("confidence").unwrap_or_default(),
        "observed_at":   r.try_get::<String, _>("observed_at").unwrap_or_default(),
    })).collect())
}

async fn fetch_jira_ticket(key: &str) -> anyhow::Result<(String, String)> {
    let base = std::env::var("JIRA_BASE_URL")
        .map_err(|_| anyhow::anyhow!("JIRA_BASE_URL not set"))?;
    let email = std::env::var("JIRA_EMAIL")
        .map_err(|_| anyhow::anyhow!("JIRA_EMAIL not set"))?;
    let token = std::env::var("JIRA_TOKEN")
        .map_err(|_| anyhow::anyhow!("JIRA_TOKEN not set"))?;

    let url = format!("{}/rest/api/2/issue/{}", base.trim_end_matches('/'), key);
    let resp = reqwest::Client::new()
        .get(&url)
        .basic_auth(&email, Some(&token))
        .send()
        .await?
        .error_for_status()?;

    let body: Value = resp.json().await?;
    let fields = &body["fields"];
    let summary = fields["summary"].as_str().unwrap_or("").to_string();
    let description = fields["description"].as_str().unwrap_or("").to_string();
    Ok((summary, description))
}

async fn resolve_spec_yaml(pool: &sqlx::AnyPool, diff_id: Option<&str>, explicit: Option<&str>) -> Result<String, ApiError> {
    use sqlx::Row;
    if let Some(s) = explicit {
        return Ok(s.to_string());
    }
    let Some(did) = diff_id else {
        return Err(ApiError::BadRequest("Provide spec_yaml or diff_id".into()));
    };
    let row = sqlx::query(
        "SELECT sv.spec_yaml FROM diff d JOIN spec_version sv ON sv.id = d.to_version WHERE d.id = ?",
    )
    .bind(did)
    .fetch_optional(pool)
    .await?;
    row.and_then(|r| r.try_get::<Option<String>, _>("spec_yaml").ok().flatten())
        .ok_or_else(|| ApiError::BadRequest("No stored spec for this diff; supply spec_yaml directly.".into()))
}

fn format_diff_test_context(changes: &[Value], evidence: &[Value]) -> String {
    let mut out = String::from("## API Contract Changes\n\n");
    for c in changes {
        let path = c["path"].as_str().unwrap_or("?");
        let kind = c["kind"].as_str().unwrap_or("?");
        let sev  = c["severity"].as_str().unwrap_or("?");
        out.push_str(&format!("- [{sev}] {kind}: {path}\n"));
    }
    if !evidence.is_empty() {
        out.push_str("\n## Active Consumer Evidence\n\n");
        for ev in evidence.iter().take(20) {
            let consumer = ev["consumer_name"].as_str()
                .or_else(|| ev["consumer_id"].as_str()).unwrap_or("?");
            let op  = ev["operation"].as_str().unwrap_or("?");
            let fp  = ev["field_path"].as_str().unwrap_or("");
            let src = ev["source_type"].as_str().unwrap_or("?");
            let conf = ev["confidence"].as_str().unwrap_or("?");
            if fp.is_empty() {
                out.push_str(&format!("- {consumer} calls {op} ({src}, {conf})\n"));
            } else {
                out.push_str(&format!("- {consumer} accesses {op} → {fp} ({src}, {conf})\n"));
            }
        }
    }
    out
}

pub(crate) fn templates_from_changes(changes: &[Value], evidence: &[Value]) -> Value {
    let mut test_cases: Vec<Value> = Vec::new();

    let evidence_ops: Vec<&str> = evidence.iter()
        .filter_map(|e| e["operation"].as_str())
        .collect();

    for change in changes {
        let kind = change["kind"].as_str().unwrap_or("");
        let raw_path = change["path"].as_str().unwrap_or("/unknown");

        let (operation, field_hint) = if let Some(idx) = raw_path.find(" \u{2192} ") {
            let op  = &raw_path[..idx];
            let fld = &raw_path[idx + " \u{2192} ".len()..];
            (op.trim(), fld.trim())
        } else {
            (raw_path.trim(), "")
        };
        let (method, route) = operation.split_once(' ').unwrap_or(("GET", operation));

        let has_evidence = evidence_ops.iter().any(|op| op.contains(route));
        let evidence_tag = if has_evidence { " [evidence]" } else { "" };

        match kind {
            "field_removed" => {
                let field = if field_hint.is_empty() { "removedField" } else { field_hint };
                test_cases.push(json!({
                    "name": format!("[HAPPY] {method} {route} — response omits `{field}`{evidence_tag}"),
                    "category": "happy_path",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": null,
                    "expected_status": 200,
                    "assertions": [
                        format!("pm.test('Field `{field}` not present in response', () => {{ pm.expect(pm.response.json()).to.not.have.nested.property('{field}'); }});")
                    ]
                }));
                test_cases.push(json!({
                    "name": format!("[NEGATIVE] {method} {route} — consumer must not depend on `{field}`{evidence_tag}"),
                    "category": "negative",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": null,
                    "expected_status": 200,
                    "assertions": [
                        format!("pm.test('Status 200 without `{field}`', () => {{ pm.response.to.have.status(200); }});")
                    ]
                }));
            }
            "required_changed" => {
                let field = if field_hint.is_empty() { "requiredField" } else { field_hint };
                test_cases.push(json!({
                    "name": format!("[NEGATIVE] {method} {route} — missing required `{field}` → 422{evidence_tag}"),
                    "category": "negative",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": {},
                    "expected_status": 422,
                    "assertions": [
                        "pm.test('Missing required field returns 400 or 422', () => { pm.expect(pm.response.code).to.be.oneOf([400, 422]); });"
                    ]
                }));
                test_cases.push(json!({
                    "name": format!("[HAPPY] {method} {route} — with required `{field}` → 2xx{evidence_tag}"),
                    "category": "happy_path",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": { field: "test-value" },
                    "expected_status": 200,
                    "assertions": [
                        "pm.test('With required field status is 2xx', () => { pm.response.to.be.success; });"
                    ]
                }));
            }
            "enum_value_removed" => {
                let field = if field_hint.is_empty() { "enumField" } else { field_hint };
                test_cases.push(json!({
                    "name": format!("[NEGATIVE] {method} {route} — removed enum value for `{field}` → 422{evidence_tag}"),
                    "category": "negative",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": { field: "REMOVED_VALUE" },
                    "expected_status": 422,
                    "assertions": [
                        "pm.test('Removed enum value returns 400 or 422', () => { pm.expect(pm.response.code).to.be.oneOf([400, 422]); });"
                    ]
                }));
            }
            "operation_removed" | "response_removed" => {
                test_cases.push(json!({
                    "name": format!("[NEGATIVE] {method} {route} — operation removed → 404 or 405{evidence_tag}"),
                    "category": "negative",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": null,
                    "expected_status": 404,
                    "assertions": [
                        "pm.test('Removed operation returns 404 or 405', () => { pm.expect(pm.response.code).to.be.oneOf([404, 405]); });"
                    ]
                }));
            }
            "type_changed" | "nullability_changed" => {
                let field = if field_hint.is_empty() { "changedField" } else { field_hint };
                test_cases.push(json!({
                    "name": format!("[NEGATIVE] {method} {route} — wrong type for `{field}` → 422{evidence_tag}"),
                    "category": "negative",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": { field: null },
                    "expected_status": 422,
                    "assertions": [
                        "pm.test('Null where non-nullable returns 400 or 422', () => { pm.expect(pm.response.code).to.be.oneOf([400, 422]); });"
                    ]
                }));
                test_cases.push(json!({
                    "name": format!("[HAPPY] {method} {route} — valid type for `{field}` → 2xx{evidence_tag}"),
                    "category": "happy_path",
                    "method": method,
                    "path": route,
                    "path_params": {},
                    "query_params": {},
                    "body": { field: "valid-value" },
                    "expected_status": 200,
                    "assertions": [
                        "pm.test('Valid type value accepted', () => { pm.response.to.be.success; });"
                    ]
                }));
            }
            _ => {}
        }
    }

    if test_cases.is_empty() {
        test_cases.push(json!({
            "name": "[HAPPY] Smoke test — service is reachable",
            "category": "happy_path",
            "method": "GET",
            "path": "/",
            "path_params": {},
            "query_params": {},
            "body": null,
            "expected_status": 200,
            "assertions": ["pm.test('Service reachable', () => { pm.response.to.have.status(200); });"]
        }));
    }

    json!({
        "collection_name": "Contract Compliance Tests",
        "test_cases": test_cases,
    })
}

async fn call_ai_for_tests_from_diff(context: &str, spec_yaml: &str) -> anyhow::Result<Value> {
    let spec_excerpt = if spec_yaml.len() > 30_000 { &spec_yaml[..30_000] } else { spec_yaml };

    let prompt = format!(
        r#"You are a QA engineer generating Postman API tests from an API contract diff.

## Contract Changes and Consumer Evidence
{context}

## OpenAPI Specification (head version)
```yaml
{spec_excerpt}
```

## Task
Generate targeted API test cases that verify the above changes are handled correctly:
1. Happy-path tests — confirm the new contract shape works
2. Negative tests — confirm removed/changed behaviour is no longer accepted

Rules:
- Use {{{{baseUrl}}}} as the host placeholder and {{{{authToken}}}} for bearer auth
- Each assertion is a complete valid JavaScript pm.test() statement on a single line
- Aim for 2–3 tests per breaking change
- Return ONLY valid JSON — no markdown fences, no surrounding text

Required JSON format:
{{
  "collection_name": "Contract Compliance Tests",
  "test_cases": [
    {{
      "name": "[HAPPY] or [NEGATIVE] description",
      "category": "happy_path",
      "method": "GET",
      "path": "/v1/resource",
      "path_params": {{}},
      "query_params": {{}},
      "body": null,
      "expected_status": 200,
      "assertions": ["pm.test('...', () => {{ ... }});"]
    }}
  ]
}}"#
    );

    let raw_text = detect_provider()
        .ok_or_else(|| anyhow::anyhow!("No AI provider configured (set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GITHUB_COPILOT_TOKEN)"))?
        .complete(&prompt, 4096)
        .await
        .ok_or_else(|| anyhow::anyhow!("AI provider call failed"))?;

    let start = raw_text.find('{').ok_or_else(|| anyhow::anyhow!("no JSON in response"))?;
    let end = raw_text.rfind('}').ok_or_else(|| anyhow::anyhow!("no JSON in response"))?;
    Ok(serde_json::from_str(&raw_text[start..=end])?)
}

async fn call_ai_for_tests(
    jira_summary: &str,
    jira_description: &str,
    spec_yaml: &str,
) -> anyhow::Result<Value> {
    let spec_excerpt = if spec_yaml.len() > 40_000 { &spec_yaml[..40_000] } else { spec_yaml };

    let prompt = format!(
        r#"You are a QA engineer generating Postman API tests from a Jira ticket and an OpenAPI spec.

## Jira Ticket
Title: {jira_summary}
Description:
{jira_description}

## OpenAPI Specification
```yaml
{spec_excerpt}
```

## Task
Generate API test cases:
1. Happy-path tests — valid inputs satisfying the ticket's acceptance criteria
2. Negative tests — missing required fields (→ 400/422), wrong types (→ 400), unauthorized (→ 401), not-found (→ 404)

Rules:
- Use {{{{baseUrl}}}} as the host placeholder and {{{{authToken}}}} for bearer auth
- Each assertion is a complete valid JavaScript pm.test() statement on a single line
- Aim for 4–6 happy-path and 4–8 negative tests
- Return ONLY valid JSON — no markdown fences, no surrounding text

Required JSON format:
{{
  "collection_name": "TICKET-KEY — Short Title",
  "test_cases": [
    {{
      "name": "Happy Path — create resource",
      "category": "happy_path",
      "method": "POST",
      "path": "/v1/resource",
      "path_params": {{}},
      "query_params": {{}},
      "body": {{"field": "value"}},
      "expected_status": 201,
      "assertions": [
        "pm.test('Response has id', () => {{ pm.expect(pm.response.json()).to.have.property('id'); }});"
      ]
    }}
  ]
}}"#
    );

    let raw_text = detect_provider()
        .ok_or_else(|| anyhow::anyhow!("No AI provider configured (set ANTHROPIC_API_KEY, OPENAI_API_KEY, or GITHUB_COPILOT_TOKEN)"))?
        .complete(&prompt, 4096)
        .await
        .ok_or_else(|| anyhow::anyhow!("AI provider call failed"))?;

    let start = raw_text.find('{').ok_or_else(|| anyhow::anyhow!("no JSON in response"))?;
    let end = raw_text.rfind('}').ok_or_else(|| anyhow::anyhow!("no JSON in response"))?;
    let suite: Value = serde_json::from_str(&raw_text[start..=end])?;
    Ok(suite)
}
