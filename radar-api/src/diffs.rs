use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::{JwtClaims, assert_org_access};
use crate::errors::ApiError;
use crate::utils::apply_evolution_rules;
use crate::PaginationParams;

#[derive(serde::Deserialize)]
pub(crate) struct CompareSpecsBody {
    pub(crate) base_spec: String,
    pub(crate) head_spec: String,
    pub(crate) spec_format: String,
    pub(crate) base_ref: String,
    pub(crate) head_ref: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct CreateDiffBody {
    pub(crate) service_name: String,
    pub(crate) repo_url: String,
    pub(crate) owner_team: String,
    pub(crate) from_git_ref: String,
    pub(crate) to_git_ref: String,
    pub(crate) pr_url: Option<String>,
    pub(crate) spec_format: String,
    pub(crate) spec_yaml: Option<String>,
    #[serde(default)]
    pub(crate) changes: Vec<ChangeInput>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ChangeInput {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) severity: String,
    pub(crate) description: Option<String>,
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct BlastRadiusParams {
    pub(crate) max_age_days: Option<u32>,
}

fn spec_version_id(service_id: &str, git_ref: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{service_id}:{git_ref}").as_bytes(),
    )
    .to_string()
}

// GET /v1/services/:id/diffs
pub(crate) async fn list_diffs(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    if !org_id.is_empty() {
        let svc_org: Option<String> = sqlx::query_scalar("SELECT org_id FROM service WHERE id = ?")
            .bind(&service_id)
            .fetch_optional(&pool)
            .await?;
        if let Some(svc_org_id) = svc_org {
            assert_org_access(&svc_org_id, &org_id, &format!("service {service_id}"))?;
        }
    }

    let rows = sqlx::query(
        r#"
        SELECT
            d.id          AS diff_id,
            sv_from.git_ref AS from_git_ref,
            sv_to.git_ref   AS to_git_ref,
            d.pr_url,
            d.created_at,
            (
                SELECT COUNT(*)
                FROM change c
                WHERE c.diff_id = d.id
                  AND c.severity = 'breaking'
            ) AS breaking_count
        FROM diff d
        JOIN spec_version sv_from ON sv_from.id = d.from_version
        JOIN spec_version sv_to   ON sv_to.id   = d.to_version
        JOIN service s            ON s.id        = sv_to.service_id
        WHERE (sv_from.service_id = ? OR sv_to.service_id = ?)
        ORDER BY d.created_at DESC
        "#,
    )
    .bind(&service_id)
    .bind(&service_id)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            let breaking_count: i64 = row.try_get("breaking_count").unwrap_or(0);
            json!({
                "id":             row.get::<String, _>("diff_id"),
                "from_git_ref":   row.get::<String, _>("from_git_ref"),
                "to_git_ref":     row.get::<String, _>("to_git_ref"),
                "pr_url":         row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
                "created_at":     row.get::<String, _>("created_at"),
                "breaking_count": breaking_count,
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// POST /v1/services/:id/diffs
pub(crate) async fn create_diff(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateDiffBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.from_git_ref.is_empty() {
        return Err(ApiError::BadRequest("from_git_ref is required".into()));
    }
    if body.to_git_ref.is_empty() {
        return Err(ApiError::BadRequest("to_git_ref is required".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name       = excluded.name,
            repo_url   = excluded.repo_url,
            owner_team = excluded.owner_team,
            spec_format = excluded.spec_format
        "#,
    )
    .bind(&service_id)
    .bind(&body.service_name)
    .bind(&body.repo_url)
    .bind(&body.owner_team)
    .bind(&body.spec_format)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    let from_version_id = spec_version_id(&service_id, &body.from_git_ref);
    sqlx::query(
        r#"
        INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(&from_version_id)
    .bind(&service_id)
    .bind(&body.from_git_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .execute(&pool)
    .await?;

    let to_version_id = spec_version_id(&service_id, &body.to_git_ref);
    sqlx::query(
        r#"
        INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            spec_yaml = COALESCE(excluded.spec_yaml, spec_version.spec_yaml)
        "#,
    )
    .bind(&to_version_id)
    .bind(&service_id)
    .bind(&body.to_git_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .bind(&body.spec_yaml)
    .execute(&pool)
    .await?;

    {
        use sqlx::Row;
        let existing = sqlx::query(
            "SELECT id FROM diff WHERE from_version = ? AND to_version = ?",
        )
        .bind(&from_version_id)
        .bind(&to_version_id)
        .fetch_optional(&pool)
        .await?;

        if let Some(row) = existing {
            let existing_id: String = row.try_get("id").map_err(ApiError::Db)?;
            return Ok((
                StatusCode::OK,
                Json(json!({
                    "id":           existing_id,
                    "from_version": from_version_id,
                    "to_version":   to_version_id,
                    "cached":       true,
                })),
            ));
        }
    }

    let diff_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO diff (id, from_version, to_version, pr_url, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&diff_id)
    .bind(&from_version_id)
    .bind(&to_version_id)
    .bind(&body.pr_url)
    .bind(&now)
    .execute(&pool)
    .await?;

    for change in &body.changes {
        let change_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO change (id, diff_id, path, kind, severity, description)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&change_id)
        .bind(&diff_id)
        .bind(&change.path)
        .bind(&change.kind)
        .bind(&change.severity)
        .bind(&change.description)
        .execute(&pool)
        .await?;
    }

    metrics::counter!("radar_diffs_created_total").increment(1);

    // Fire webhook events in background (non-blocking)
    {
        let pool2 = pool.clone();
        let did = diff_id.clone();
        let oid = org_id.clone();
        tokio::spawn(async move {
            crate::audit::record_event(&pool2, &oid, "system", "diff.created", Some("diff"), Some(&did), None).await;
            crate::webhooks::dispatch_diff_event(pool2, did, oid).await;
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":           diff_id,
            "from_version": from_version_id,
            "to_version":   to_version_id,
            "created_at":   now,
        })),
    ))
}

// GET /v1/diffs/:id
pub(crate) async fn get_diff(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let caller_org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let row = sqlx::query(
        r#"
        SELECT d.id, sv_from.git_ref AS from_git_ref, sv_to.git_ref AS to_git_ref,
               d.pr_url, d.created_at, d.share_token, sv_to.spec_yaml, s.org_id AS service_org_id
        FROM diff d
        JOIN spec_version sv_from ON sv_from.id = d.from_version
        JOIN spec_version sv_to   ON sv_to.id   = d.to_version
        JOIN service s            ON s.id        = sv_to.service_id
        WHERE d.id = ?
        "#,
    )
    .bind(&diff_id)
    .fetch_optional(&pool)
    .await?;

    let row = match row {
        None => return Err(ApiError::NotFound(format!("diff {diff_id} not found"))),
        Some(r) => r,
    };

    let row_org_id: String = row.try_get("service_org_id").unwrap_or_default();
    assert_org_access(&row_org_id, &caller_org_id, &format!("diff {diff_id}"))?;

    // Generate share token if absent (back-fill)
    let share_token: Option<String> = row.try_get("share_token").ok().flatten();
    let share_token = if let Some(t) = share_token {
        t
    } else {
        let token = Uuid::new_v5(&Uuid::NAMESPACE_URL, diff_id.as_bytes()).to_string();
        let _ = sqlx::query("UPDATE diff SET share_token = ? WHERE id = ?")
            .bind(&token)
            .bind(&diff_id)
            .execute(&pool)
            .await;
        token
    };

    let change_rows = sqlx::query(
        r#"
        SELECT path, kind, severity, description
        FROM change
        WHERE diff_id = ?
        ORDER BY path, kind
        "#,
    )
    .bind(&diff_id)
    .fetch_all(&pool)
    .await?;

    let raw_changes: Vec<Value> = change_rows
        .iter()
        .map(|c| {
            json!({
                "path":        c.get::<String, _>("path"),
                "kind":        c.get::<String, _>("kind"),
                "severity":    c.get::<String, _>("severity"),
                "description": c.try_get::<Option<String>, _>("description").unwrap_or(None),
            })
        })
        .collect();

    let rule_rows = sqlx::query(
        "SELECT id, name, path_pattern, change_kind, severity_override
         FROM evolution_rule
         WHERE org_id = ? AND enabled = 1
         ORDER BY created_at ASC",
    )
    .bind(&caller_org_id)
    .fetch_all(&pool)
    .await?;

    let rules: Vec<(String, String, Option<String>, String, String)> = rule_rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("id"),
                r.get::<String, _>("name"),
                r.try_get::<Option<String>, _>("path_pattern").unwrap_or(None),
                r.get::<String, _>("change_kind"),
                r.get::<String, _>("severity_override"),
            )
        })
        .collect();

    let changes = apply_evolution_rules(raw_changes, &rules);

    Ok((
        StatusCode::OK,
        Json(json!({
            "id":           row.get::<String, _>("id"),
            "from_git_ref": row.get::<String, _>("from_git_ref"),
            "to_git_ref":   row.get::<String, _>("to_git_ref"),
            "pr_url":       row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
            "created_at":   row.get::<String, _>("created_at"),
            "spec_yaml":    row.try_get::<Option<String>, _>("spec_yaml").unwrap_or(None),
            "share_token":  share_token,
            "changes":      changes,
        })),
    ))
}

// GET /share/:token — public, unauthenticated diff view (K-4)
pub(crate) async fn get_shared_diff(
    Path(token): Path<String>,
    State(pool): State<sqlx::AnyPool>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let diff_row = sqlx::query(
        r#"SELECT d.id, sv_from.git_ref AS from_git_ref, sv_to.git_ref AS to_git_ref,
                  d.pr_url, d.created_at, s.name AS service_name
           FROM diff d
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           JOIN service s            ON s.id        = sv_to.service_id
           WHERE d.share_token = ?"#,
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await?;

    let diff_row = diff_row.ok_or_else(|| ApiError::NotFound("shared diff not found".into()))?;
    let diff_id: String = diff_row.get("id");

    let change_rows = sqlx::query(
        "SELECT path, kind, severity, description FROM change WHERE diff_id = ? ORDER BY path, kind",
    )
    .bind(&diff_id)
    .fetch_all(&pool)
    .await?;

    let changes: Vec<serde_json::Value> = change_rows.iter().map(|c| {
        json!({
            "path":        c.get::<String, _>("path"),
            "kind":        c.get::<String, _>("kind"),
            "severity":    c.get::<String, _>("severity"),
            "description": c.try_get::<Option<String>, _>("description").unwrap_or(None),
        })
    }).collect();

    Ok(Json(json!({
        "id":           diff_id,
        "service_name": diff_row.get::<String, _>("service_name"),
        "from_git_ref": diff_row.get::<String, _>("from_git_ref"),
        "to_git_ref":   diff_row.get::<String, _>("to_git_ref"),
        "pr_url":       diff_row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
        "created_at":   diff_row.get::<String, _>("created_at"),
        "changes":      changes,
    })))
}

// GET /v1/diffs/:id/blast-radius?max_age_days=N
pub(crate) async fn blast_radius(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Query(params): Query<BlastRadiusParams>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let caller_org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let diff_row = sqlx::query("SELECT id, from_version, to_version FROM diff WHERE id = ?")
        .bind(&diff_id)
        .fetch_optional(&pool)
        .await?;

    let diff_row = match diff_row {
        Some(r) => r,
        None => return Err(ApiError::NotFound(format!("diff {diff_id} not found"))),
    };

    let to_version: String = diff_row.try_get("to_version").map_err(ApiError::Db)?;

    let sv_row = sqlx::query(
        "SELECT sv.service_id, s.org_id FROM spec_version sv JOIN service s ON s.id = sv.service_id WHERE sv.id = ?",
    )
    .bind(&to_version)
    .fetch_optional(&pool)
    .await?;

    let (service_id, svc_org_id): (String, String) = match sv_row {
        Some(r) => (
            r.try_get("service_id").map_err(ApiError::Db)?,
            r.try_get("org_id").unwrap_or_default(),
        ),
        None => {
            return Ok((
                StatusCode::OK,
                Json(json!({
                    "diff_id": diff_id,
                    "service_id": "",
                    "lookback_days": 30,
                    "entries": [],
                })),
            ))
        }
    };

    assert_org_access(&svc_org_id, &caller_org_id, &format!("diff {diff_id}"))?;

    let change_rows = sqlx::query("SELECT path FROM change WHERE diff_id = ?")
        .bind(&diff_id)
        .fetch_all(&pool)
        .await?;

    let mut op_level_ops: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut changed_fields: Vec<(String, String)> = Vec::new();

    for row in &change_rows {
        let path: String = row.try_get("path").map_err(ApiError::Db)?;
        if let Some(arrow_pos) = path.find(" \u{2192} ") {
            let op = path[..arrow_pos].to_string();
            let after_arrow = &path[arrow_pos + " → ".len()..];
            let field = if let Some(stripped) = after_arrow.strip_prefix("response.") {
                stripped.to_string()
            } else {
                after_arrow.to_string()
            };
            changed_fields.push((op, field));
        } else {
            op_level_ops.insert(path);
        }
    }

    let field_level_only: Vec<(String, String)> = {
        let mut seen = std::collections::HashSet::new();
        changed_fields
            .iter()
            .filter(|(op, _)| !op_level_ops.contains(op.as_str()))
            .filter(|(op, fp)| seen.insert((op.clone(), fp.clone())))
            .cloned()
            .collect()
    };

    let consumer_rows = sqlx::query(
        r#"
        SELECT c.id, c.name, c.repo_url, c.owner_team, c.contact
        FROM consumer c
        JOIN subscription s ON s.consumer_id = c.id
        WHERE s.service_id = ?
        "#,
    )
    .bind(&service_id)
    .fetch_all(&pool)
    .await?;

    let lookback_days: i64 = 30;
    let cutoff_30 = (Utc::now() - Duration::days(lookback_days)).to_rfc3339();
    let cutoff_7 = (Utc::now() - Duration::days(7)).to_rfc3339();

    let mut entries: Vec<Value> = Vec::new();

    for consumer_row in &consumer_rows {
        let consumer_id: String = consumer_row.try_get("id").map_err(ApiError::Db)?;
        let consumer_name: String = consumer_row.try_get("name").map_err(ApiError::Db)?;
        let consumer_repo: String = consumer_row.try_get("repo_url").map_err(ApiError::Db)?;
        let consumer_team: String = consumer_row.try_get("owner_team").map_err(ApiError::Db)?;
        let consumer_contact: String = consumer_row.try_get("contact").map_err(ApiError::Db)?;

        let mut evidence_items: Vec<Value> = Vec::new();

        if !op_level_ops.is_empty() || !field_level_only.is_empty() {
            let mut sql = String::from(
                "SELECT operation, field_path, recorded_at FROM usage_event \
                 WHERE consumer_id = ? AND service_id = ? AND recorded_at >= ? AND (",
            );
            let mut first = true;
            for _ in &op_level_ops {
                if !first { sql.push_str(" OR "); }
                sql.push_str("operation = ?");
                first = false;
            }
            for _ in &field_level_only {
                if !first { sql.push_str(" OR "); }
                sql.push_str("(operation = ? AND field_path = ?)");
                first = false;
            }
            sql.push_str(") ORDER BY recorded_at DESC LIMIT 5");

            let mut q = sqlx::query(&sql)
                .bind(&consumer_id)
                .bind(&service_id)
                .bind(&cutoff_30);
            for op in &op_level_ops {
                q = q.bind(op);
            }
            for (op, fp) in &field_level_only {
                q = q.bind(op);
                q = q.bind(fp);
            }

            for row in q.fetch_all(&pool).await? {
                use sqlx::Row as _;
                let op: String = row.try_get("operation").unwrap_or_default();
                let fp: Option<String> = row.try_get("field_path").ok().flatten();
                let ts: String = row.try_get("recorded_at").unwrap_or_default();
                evidence_items.push(json!({
                    "kind":        "runtime_usage",
                    "operation":   op,
                    "field_path":  fp,
                    "recorded_at": ts,
                }));
            }
        }

        let changed_field_paths: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            changed_fields
                .iter()
                .map(|(_, fp)| fp.clone())
                .filter(|fp| !fp.is_empty() && seen.insert(fp.clone()))
                .collect()
        };

        if !op_level_ops.is_empty() || !changed_field_paths.is_empty() {
            let mut sql = String::from(
                "SELECT operation, field_path, file_path, line_number, last_seen_at \
                 FROM call_site WHERE consumer_id = ? AND service_id = ? AND (",
            );
            let mut first = true;
            for _ in &op_level_ops {
                if !first { sql.push_str(" OR "); }
                sql.push_str("operation = ?");
                first = false;
            }
            for _ in &changed_field_paths {
                if !first { sql.push_str(" OR "); }
                sql.push_str("field_path = ?");
                first = false;
            }
            sql.push_str(") ORDER BY last_seen_at DESC LIMIT 5");

            let mut q = sqlx::query(&sql).bind(&consumer_id).bind(&service_id);
            for op in &op_level_ops {
                q = q.bind(op);
            }
            for fp in &changed_field_paths {
                q = q.bind(fp);
            }

            for row in q.fetch_all(&pool).await? {
                use sqlx::Row as _;
                let op: String = row.try_get("operation").unwrap_or_default();
                let fp: Option<String> = row.try_get("field_path").ok().flatten();
                let fp_val = fp.filter(|s| !s.is_empty());
                let file: String = row.try_get("file_path").unwrap_or_default();
                let line: i64 = row.try_get("line_number").unwrap_or(0);
                let ts: String = row.try_get("last_seen_at").unwrap_or_default();
                evidence_items.push(json!({
                    "kind":         "call_site",
                    "operation":    op,
                    "field_path":   fp_val,
                    "file_path":    file,
                    "line_number":  line,
                    "last_seen_at": ts,
                }));
            }
        }

        if let Some(days) = params.max_age_days {
            let cutoff_age = (Utc::now() - Duration::days(i64::from(days))).to_rfc3339();
            evidence_items.retain(|e| {
                let ts = e["recorded_at"].as_str()
                    .or_else(|| e["last_seen_at"].as_str())
                    .unwrap_or("");
                !ts.is_empty() && ts >= cutoff_age.as_str()
            });
        }

        if evidence_items.is_empty() {
            continue;
        }

        {
            let now_str = Utc::now().to_rfc3339();
            for item in &evidence_items {
                let ev_id = Uuid::new_v4().to_string();
                let source_type = if item["kind"] == "runtime_usage" {
                    "runtime_usage"
                } else {
                    "static_call_site"
                };
                let observed_at = item["recorded_at"].as_str()
                    .or_else(|| item["last_seen_at"].as_str())
                    .unwrap_or(now_str.as_str());
                let item_confidence = if source_type == "runtime_usage" {
                    if observed_at >= cutoff_7.as_str() { "high" } else { "medium" }
                } else {
                    let op = item["operation"].as_str().unwrap_or("").trim();
                    if !op.is_empty() { "medium" } else { "low" }
                };
                sqlx::query(
                    "INSERT INTO impact_evidence \
                     (id, org_id, diff_id, producer_service_id, consumer_id, source_type, \
                      operation, field_path, confidence, file_path, line_number, observed_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&ev_id)
                .bind(&svc_org_id)
                .bind(&diff_id)
                .bind(&service_id)
                .bind(&consumer_id)
                .bind(source_type)
                .bind(item["operation"].as_str().unwrap_or(""))
                .bind(item["field_path"].as_str().unwrap_or(""))
                .bind(item_confidence)
                .bind(item["file_path"].as_str())
                .bind(item["line_number"].as_i64())
                .bind(observed_at)
                .execute(&pool)
                .await?;
            }
        }

        evidence_items.sort_by_key(|e| if e["kind"] == "runtime_usage" { 0u8 } else { 1u8 });

        let has_runtime_usage = evidence_items.iter().any(|e| e["kind"] == "runtime_usage");
        let has_call_site = evidence_items.iter().any(|e| e["kind"] == "call_site");

        let usage_last_seen: Option<String> = evidence_items.iter()
            .filter(|e| e["kind"] == "runtime_usage")
            .filter_map(|e| e["recorded_at"].as_str().map(|s| s.to_string()))
            .max();
        let call_site_last_seen: Option<String> = evidence_items.iter()
            .filter(|e| e["kind"] == "call_site")
            .filter_map(|e| e["last_seen_at"].as_str().map(|s| s.to_string()))
            .max();

        let confidence = if let Some(ref ts) = usage_last_seen {
            if ts.as_str() >= cutoff_7.as_str() {
                "high"
            } else {
                "medium"
            }
        } else {
            "low"
        };

        let last_seen = usage_last_seen
            .or(call_site_last_seen)
            .unwrap_or_default();

        entries.push(json!({
            "consumer": {
                "id":         consumer_id,
                "name":       consumer_name,
                "repo_url":   consumer_repo,
                "owner_team": consumer_team,
                "contact":    consumer_contact,
            },
            "confidence":        confidence,
            "last_seen":         last_seen,
            "has_runtime_usage": has_runtime_usage,
            "has_call_site":     has_call_site,
            "evidence":          evidence_items,
        }));
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "diff_id":      diff_id,
            "service_id":   service_id,
            "lookback_days": lookback_days,
            "entries":      entries,
        })),
    ))
}

// GET /v1/diffs
pub(crate) async fn list_all_diffs(
    State(pool): State<sqlx::AnyPool>,
    Query(page): Query<PaginationParams>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let limit = page.limit.clamp(1, 200);
    let offset = page.offset.max(0);

    let base_query = r#"
        SELECT
            d.id            AS diff_id,
            sv_from.git_ref AS from_git_ref,
            sv_to.git_ref   AS to_git_ref,
            s.id            AS service_id,
            s.name          AS service_name,
            d.pr_url,
            d.created_at,
            (SELECT COUNT(*) FROM change c WHERE c.diff_id = d.id AND c.severity = 'breaking')           AS breaking_count,
            (SELECT COUNT(*) FROM change c WHERE c.diff_id = d.id AND c.severity = 'non_breaking_risky') AS risky_count,
            (SELECT COUNT(*) FROM change c WHERE c.diff_id = d.id AND c.severity = 'safe')               AS safe_count
        FROM diff d
        JOIN spec_version sv_from ON sv_from.id = d.from_version
        JOIN spec_version sv_to   ON sv_to.id   = d.to_version
        JOIN service s            ON s.id        = sv_to.service_id
    "#;

    let rows = if !org_id.is_empty() {
        sqlx::query(&format!("{base_query} WHERE s.org_id = ? ORDER BY d.created_at DESC LIMIT ? OFFSET ?"))
            .bind(&org_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?
    } else {
        sqlx::query(&format!("{base_query} ORDER BY d.created_at DESC LIMIT ? OFFSET ?"))
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?
    };

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":            row.get::<String, _>("diff_id"),
                "service_id":    row.get::<String, _>("service_id"),
                "service_name":  row.get::<String, _>("service_name"),
                "from_git_ref":  row.get::<String, _>("from_git_ref"),
                "to_git_ref":    row.get::<String, _>("to_git_ref"),
                "pr_url":        row.try_get::<Option<String>, _>("pr_url").unwrap_or(None),
                "created_at":    row.get::<String, _>("created_at"),
                "breaking_count": row.try_get::<i64, _>("breaking_count").unwrap_or(0),
                "risky_count":    row.try_get::<i64, _>("risky_count").unwrap_or(0),
                "safe_count":     row.try_get::<i64, _>("safe_count").unwrap_or(0),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// POST /v1/services/:id/diffs/compare
// Accepts raw spec strings, parses and diffs them server-side, persists the result.
pub(crate) async fn compare_specs(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CompareSpecsBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.base_ref.is_empty() {
        return Err(ApiError::BadRequest("base_ref is required".into()));
    }
    if body.head_ref.is_empty() {
        return Err(ApiError::BadRequest("head_ref is required".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let format = body.spec_format.to_lowercase();

    // Parse both specs and compute the diff.
    let changes: Vec<radar_core::diff::DiffChange> = match format.as_str() {
        "graphql" | "gql" => {
            let base_map = radar_core::graphql::parse_graphql(&body.base_spec)
                .map_err(|e| ApiError::UnprocessableEntity {
                    error: "parse_error".into(),
                    detail: e.to_string(),
                    spec: "base".into(),
                })?;
            let head_map = radar_core::graphql::parse_graphql(&body.head_spec)
                .map_err(|e| ApiError::UnprocessableEntity {
                    error: "parse_error".into(),
                    detail: e.to_string(),
                    spec: "head".into(),
                })?;
            radar_core::graphql::diff_graphql(&base_map, &head_map)
        }
        "protobuf" | "proto" => {
            let base_schema = radar_core::proto::parse_proto(&body.base_spec)
                .map_err(|e| ApiError::UnprocessableEntity {
                    error: "parse_error".into(),
                    detail: e.to_string(),
                    spec: "base".into(),
                })?;
            let head_schema = radar_core::proto::parse_proto(&body.head_spec)
                .map_err(|e| ApiError::UnprocessableEntity {
                    error: "parse_error".into(),
                    detail: e.to_string(),
                    spec: "head".into(),
                })?;
            radar_core::proto::diff_proto(&base_schema, &head_schema)
        }
        _ => {
            let base_parsed = radar_core::diff::parse_openapi(&body.base_spec)
                .map_err(|e| ApiError::UnprocessableEntity {
                    error: "parse_error".into(),
                    detail: e.to_string(),
                    spec: "base".into(),
                })?;
            let head_parsed = radar_core::diff::parse_openapi(&body.head_spec)
                .map_err(|e| ApiError::UnprocessableEntity {
                    error: "parse_error".into(),
                    detail: e.to_string(),
                    spec: "head".into(),
                })?;
            radar_core::diff::diff_openapi(&base_parsed, &head_parsed)
        }
    };

    let now = Utc::now().to_rfc3339();

    // Upsert the service record so the endpoint is self-contained.
    sqlx::query(
        "INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET spec_format = excluded.spec_format",
    )
    .bind(&service_id)
    .bind(&service_id)
    .bind("")
    .bind("")
    .bind(&body.spec_format)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    let from_version_id = spec_version_id(&service_id, &body.base_ref);
    sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&from_version_id)
    .bind(&service_id)
    .bind(&body.base_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .bind(&body.base_spec)
    .execute(&pool)
    .await?;

    let to_version_id = spec_version_id(&service_id, &body.head_ref);
    sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET spec_yaml = COALESCE(excluded.spec_yaml, spec_version.spec_yaml)",
    )
    .bind(&to_version_id)
    .bind(&service_id)
    .bind(&body.head_ref)
    .bind(&now)
    .bind(&body.spec_format)
    .bind(&body.head_spec)
    .execute(&pool)
    .await?;

    // Re-use an existing diff for the same (from, to) pair.
    {
        use sqlx::Row;
        let existing = sqlx::query(
            "SELECT id FROM diff WHERE from_version = ? AND to_version = ?",
        )
        .bind(&from_version_id)
        .bind(&to_version_id)
        .fetch_optional(&pool)
        .await?;

        if let Some(row) = existing {
            let existing_id: String = row.try_get("id").map_err(ApiError::Db)?;
            let breaking_count = changes.iter()
                .filter(|c| c.severity == radar_core::models::Severity::Breaking)
                .count() as i64;
            return Ok((
                StatusCode::OK,
                Json(json!({
                    "diff_id":        existing_id,
                    "changes_count":  changes.len() as i64,
                    "breaking_count": breaking_count,
                    "cached":         true,
                })),
            ));
        }
    }

    let diff_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO diff (id, from_version, to_version, pr_url, created_at) \
         VALUES (?, ?, ?, NULL, ?)",
    )
    .bind(&diff_id)
    .bind(&from_version_id)
    .bind(&to_version_id)
    .bind(&now)
    .execute(&pool)
    .await?;

    let mut breaking_count: i64 = 0;
    for change in &changes {
        let change_id = Uuid::new_v4().to_string();
        let sev_str = match change.severity {
            radar_core::models::Severity::Breaking => { breaking_count += 1; "breaking" }
            radar_core::models::Severity::NonBreakingRisky => "non_breaking_risky",
            radar_core::models::Severity::Safe => "safe",
        };
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&change_id)
        .bind(&diff_id)
        .bind(&change.path)
        .bind(change.kind.as_str())
        .bind(sev_str)
        .bind(&change.description)
        .execute(&pool)
        .await?;
    }

    metrics::counter!("radar_diffs_created_total").increment(1);

    // Fire webhook events in background (non-blocking)
    {
        let pool2 = pool.clone();
        let did = diff_id.clone();
        let oid = org_id.clone();
        tokio::spawn(async move {
            crate::audit::record_event(&pool2, &oid, "system", "diff.created", Some("diff"), Some(&did), None).await;
            crate::webhooks::dispatch_diff_event(pool2, did, oid).await;
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "diff_id":        diff_id,
            "changes_count":  changes.len() as i64,
            "breaking_count": breaking_count,
        })),
    ))
}

// ── Batch compare (POST /v1/compare/batch) ────────────────────────────────────
// Accepts up to 50 {base_url, head_url} pairs. The sidecar fetches each URL
// server-side (bypassing browser CSP) and persists a diff per row.

#[derive(serde::Deserialize)]
pub(crate) struct BatchCompareItem {
    pub(crate) label: Option<String>,
    pub(crate) service_id: Option<String>,
    #[serde(default = "default_batch_format")]
    pub(crate) format: String,
    pub(crate) base_url: String,
    pub(crate) head_url: String,
}

fn default_batch_format() -> String { "openapi".to_string() }

#[derive(serde::Serialize)]
struct BatchResultItem {
    label: String,
    status: String,
    diff_id: Option<String>,
    breaking_count: i64,
    changes_count: i64,
    error: Option<String>,
}

pub(crate) async fn batch_compare(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(items): Json<Vec<BatchCompareItem>>,
) -> Result<impl IntoResponse, ApiError> {
    if items.len() > 50 {
        return Err(ApiError::BadRequest("batch too large, max 50 items".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("radar-api/batch-compare")
        .build()
        .unwrap_or_default();
    let mut results: Vec<BatchResultItem> = Vec::new();

    for item in &items {
        let label = item.label.as_deref().unwrap_or(item.base_url.as_str()).to_string();
        match run_batch_item(&pool, &http, item, &org_id, &label).await {
            Ok(r) => results.push(r),
            Err(e) => results.push(BatchResultItem {
                label,
                status: "error".into(),
                diff_id: None,
                breaking_count: 0,
                changes_count: 0,
                error: Some(e.to_string()),
            }),
        }
    }

    Ok((StatusCode::OK, Json(results)))
}

async fn run_batch_item(
    pool: &sqlx::AnyPool,
    http: &reqwest::Client,
    item: &BatchCompareItem,
    org_id: &str,
    label: &str,
) -> anyhow::Result<BatchResultItem> {
    let base_content = http.get(&item.base_url)
        .send().await.map_err(|e| anyhow::anyhow!("fetch base_url: {e}"))?
        .error_for_status().map_err(|e| anyhow::anyhow!("base_url HTTP error: {e}"))?
        .text().await.map_err(|e| anyhow::anyhow!("read base_url: {e}"))?;

    let head_content = http.get(&item.head_url)
        .send().await.map_err(|e| anyhow::anyhow!("fetch head_url: {e}"))?
        .error_for_status().map_err(|e| anyhow::anyhow!("head_url HTTP error: {e}"))?
        .text().await.map_err(|e| anyhow::anyhow!("read head_url: {e}"))?;

    let format = item.format.to_lowercase();

    let changes: Vec<radar_core::diff::DiffChange> = match format.as_str() {
        "graphql" | "gql" => {
            let bm = radar_core::graphql::parse_graphql(&base_content)
                .map_err(|e| anyhow::anyhow!("parse base graphql: {e}"))?;
            let hm = radar_core::graphql::parse_graphql(&head_content)
                .map_err(|e| anyhow::anyhow!("parse head graphql: {e}"))?;
            radar_core::graphql::diff_graphql(&bm, &hm)
        }
        "protobuf" | "proto" => {
            let bs = radar_core::proto::parse_proto(&base_content)
                .map_err(|e| anyhow::anyhow!("parse base proto: {e}"))?;
            let hs = radar_core::proto::parse_proto(&head_content)
                .map_err(|e| anyhow::anyhow!("parse head proto: {e}"))?;
            radar_core::proto::diff_proto(&bs, &hs)
        }
        _ => {
            let bp = radar_core::diff::parse_openapi(&base_content)
                .map_err(|e| anyhow::anyhow!("parse base openapi: {e}"))?;
            let hp = radar_core::diff::parse_openapi(&head_content)
                .map_err(|e| anyhow::anyhow!("parse head openapi: {e}"))?;
            radar_core::diff::diff_openapi(&bp, &hp)
        }
    };

    // Stable service_id: use provided, or derive from label so the same label
    // always maps to the same service entry in the DB.
    let service_id = item.service_id.clone().unwrap_or_else(|| {
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("batch:{label}").as_bytes()).to_string()
    });

    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO service (id, name, repo_url, owner_team, spec_format, org_id) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET spec_format = excluded.spec_format",
    )
    .bind(&service_id).bind(label).bind("").bind("").bind(&format).bind(org_id)
    .execute(pool).await.map_err(|e| anyhow::anyhow!("upsert service: {e}"))?;

    // Use URL strings as git refs — keeps spec_version IDs stable across reruns.
    let from_ver = spec_version_id(&service_id, &item.base_url);
    sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) \
         VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(&from_ver).bind(&service_id).bind(&item.base_url)
    .bind(&now).bind(&format).bind(&base_content)
    .execute(pool).await.map_err(|e| anyhow::anyhow!("insert base spec_version: {e}"))?;

    let to_ver = spec_version_id(&service_id, &item.head_url);
    sqlx::query(
        "INSERT INTO spec_version (id, service_id, git_ref, captured_at, spec_format, spec_yaml) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET spec_yaml = COALESCE(excluded.spec_yaml, spec_version.spec_yaml)",
    )
    .bind(&to_ver).bind(&service_id).bind(&item.head_url)
    .bind(&now).bind(&format).bind(&head_content)
    .execute(pool).await.map_err(|e| anyhow::anyhow!("insert head spec_version: {e}"))?;

    // Re-use an existing diff for the same (from, to) pair.
    if let Some(row) = sqlx::query("SELECT id FROM diff WHERE from_version = ? AND to_version = ?")
        .bind(&from_ver).bind(&to_ver)
        .fetch_optional(pool).await.map_err(|e| anyhow::anyhow!("select diff: {e}"))?
    {
        use sqlx::Row;
        let existing_id: String = row.try_get("id").map_err(|e| anyhow::anyhow!("get id: {e}"))?;
        let bc = changes.iter()
            .filter(|c| c.severity == radar_core::models::Severity::Breaking)
            .count() as i64;
        return Ok(BatchResultItem {
            label: label.to_string(),
            status: "done".into(),
            diff_id: Some(existing_id),
            breaking_count: bc,
            changes_count: changes.len() as i64,
            error: None,
        });
    }

    let diff_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO diff (id, from_version, to_version, pr_url, created_at) VALUES (?, ?, ?, NULL, ?)")
        .bind(&diff_id).bind(&from_ver).bind(&to_ver).bind(&now)
        .execute(pool).await.map_err(|e| anyhow::anyhow!("insert diff: {e}"))?;

    let mut breaking_count: i64 = 0;
    for change in &changes {
        let sev = match change.severity {
            radar_core::models::Severity::Breaking => { breaking_count += 1; "breaking" }
            radar_core::models::Severity::NonBreakingRisky => "non_breaking_risky",
            radar_core::models::Severity::Safe => "safe",
        };
        sqlx::query(
            "INSERT INTO change (id, diff_id, path, kind, severity, description) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string()).bind(&diff_id)
        .bind(&change.path).bind(change.kind.as_str()).bind(sev).bind(&change.description)
        .execute(pool).await.map_err(|e| anyhow::anyhow!("insert change: {e}"))?;
    }

    metrics::counter!("radar_diffs_created_total").increment(1);

    Ok(BatchResultItem {
        label: label.to_string(),
        status: "done".into(),
        diff_id: Some(diff_id),
        breaking_count,
        changes_count: changes.len() as i64,
        error: None,
    })
}
