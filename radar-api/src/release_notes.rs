use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;
use crate::auth::{JwtClaims, OrgResource, require_org_owned};
use crate::errors::ApiError;
use crate::ai_tests::load_diff_evidence;
use crate::PaginationParams;

type OrgExt = Option<axum::extract::Extension<JwtClaims>>;

fn caller_org(org: &OrgExt) -> String {
    org.as_ref().map(|e| e.org_id.clone()).unwrap_or_default()
}

#[derive(serde::Deserialize)]
pub(crate) struct CreateReleaseNoteBody {
    pub(crate) content: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct PatchStatusBody {
    pub(crate) status: String,
}

// POST /v1/diffs/:id/release-notes
pub(crate) async fn create_release_note(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
    Json(body): Json<CreateReleaseNoteBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_org_owned(&pool, OrgResource::Diff, &diff_id, &caller_org(&org)).await?;
    if body.content.is_empty() {
        return Err(ApiError::BadRequest("content is required".into()));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO release_note (id, diff_id, content, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&diff_id)
    .bind(&body.content)
    .bind(&now)
    .execute(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "diff_id": diff_id, "created_at": now }))))
}

// GET /v1/release-notes
pub(crate) async fn list_release_notes(
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
    Query(params): Query<PaginationParams>,
) -> Result<impl IntoResponse, ApiError> {
    // Clamp: a negative LIMIT dumps the whole table on SQLite and 500s on Postgres.
    let (limit, offset) =
        crate::utils::clamp_pagination(Some(params.limit), Some(params.offset));
    let org_id = caller_org(&org);
    // Org isolation: authenticated callers only see release notes for diffs whose
    // producer service belongs to their org. Empty org (desktop/no-auth) sees all.
    let base = r#"SELECT rn.id, rn.diff_id, rn.created_at, rn.status,
                  d.from_version, d.to_version,
                  sv_from.git_ref AS from_git_ref,
                  sv_to.git_ref   AS to_git_ref
           FROM release_note rn
           JOIN diff        d      ON d.id      = rn.diff_id
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           JOIN service     svc    ON svc.id     = sv_from.service_id"#;
    let rows = if org_id.is_empty() {
        sqlx::query(&format!("{base} ORDER BY rn.created_at DESC LIMIT ? OFFSET ?"))
            .bind(limit)
            .bind(offset)
            .fetch_all(&pool)
            .await?
    } else {
        sqlx::query(&format!(
            "{base} WHERE svc.org_id = ? ORDER BY rn.created_at DESC LIMIT ? OFFSET ?"
        ))
        .bind(&org_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await?
    };

    let items: Vec<Value> = rows.iter().map(|r| {
        use sqlx::Row;
        json!({
            "id":           r.get::<String, _>("id"),
            "diff_id":      r.get::<String, _>("diff_id"),
            "from_git_ref": r.get::<String, _>("from_git_ref"),
            "to_git_ref":   r.get::<String, _>("to_git_ref"),
            "status":       r.try_get::<String, _>("status").unwrap_or_else(|_| "draft".into()),
            "created_at":   r.get::<String, _>("created_at"),
        })
    }).collect();

    Ok(Json(json!(items)))
}

// GET /v1/release-notes/:id
pub(crate) async fn get_release_note(
    Path(note_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
) -> Result<impl IntoResponse, ApiError> {
    require_org_owned(&pool, OrgResource::ReleaseNote, &note_id, &caller_org(&org)).await?;
    let row = sqlx::query(
        r#"SELECT rn.id, rn.diff_id, rn.content, rn.created_at,
                  sv_from.git_ref AS from_git_ref,
                  sv_to.git_ref   AS to_git_ref
           FROM release_note rn
           JOIN diff        d      ON d.id        = rn.diff_id
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           WHERE rn.id = ?"#,
    )
    .bind(&note_id)
    .fetch_optional(&pool)
    .await?;

    match row {
        None => Err(ApiError::NotFound(format!("release note {note_id} not found"))),
        Some(r) => {
            use sqlx::Row;
            Ok(Json(json!({
                "id":           r.get::<String, _>("id"),
                "diff_id":      r.get::<String, _>("diff_id"),
                "from_git_ref": r.get::<String, _>("from_git_ref"),
                "to_git_ref":   r.get::<String, _>("to_git_ref"),
                "content":      r.get::<String, _>("content"),
                "status":       r.try_get::<String, _>("status").unwrap_or_else(|_| "draft".into()),
                "created_at":   r.get::<String, _>("created_at"),
            })))
        }
    }
}

// PATCH /v1/release-notes/:id/status
pub(crate) async fn patch_release_note_status(
    Path(note_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
    Json(body): Json<PatchStatusBody>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    require_org_owned(&pool, OrgResource::ReleaseNote, &note_id, &caller_org(&org)).await?;
    const VALID_STATUSES: &[&str] = &["draft", "reviewed", "published", "superseded"];
    if !VALID_STATUSES.contains(&body.status.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "invalid status '{}'; must be one of: {}",
            body.status,
            VALID_STATUSES.join(", ")
        )));
    }

    let row = sqlx::query("SELECT status FROM release_note WHERE id = ?")
        .bind(&note_id)
        .fetch_optional(&pool)
        .await?;

    let Some(row) = row else {
        return Err(ApiError::NotFound(format!("release note {note_id} not found")));
    };
    let current: String = row.try_get("status").unwrap_or_else(|_| "draft".into());

    let allowed_next = match current.as_str() {
        "draft"       => &["reviewed", "superseded"][..],
        "reviewed"    => &["published", "draft", "superseded"][..],
        "published"   => &["superseded"][..],
        "superseded"  => &[][..],
        _             => &["draft"][..],
    };
    if !allowed_next.contains(&body.status.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "transition '{current}' → '{}' is not allowed",
            body.status
        )));
    }

    sqlx::query("UPDATE release_note SET status = ? WHERE id = ?")
        .bind(&body.status)
        .bind(&note_id)
        .execute(&pool)
        .await?;

    Ok(Json(json!({ "id": note_id, "status": body.status })))
}

// GET /v1/diffs/:id/migration-guide?consumer_id=xxx
pub(crate) async fn get_migration_guide(
    Path(diff_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;
    require_org_owned(&pool, OrgResource::Diff, &diff_id, &caller_org(&org)).await?;
    let consumer_id = params.get("consumer_id").map(String::as_str);

    let diff_row = sqlx::query(
        r#"SELECT d.id, sv_from.git_ref AS from_ref, sv_to.git_ref AS to_ref,
                  s.name AS service_name
           FROM diff d
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           JOIN service s ON s.id = sv_from.service_id
           WHERE d.id = ?"#,
    )
    .bind(&diff_id)
    .fetch_optional(&pool)
    .await?;
    let Some(dr) = diff_row else {
        return Err(ApiError::NotFound(format!("diff {diff_id} not found")));
    };
    let from_ref: String = dr.try_get("from_ref").unwrap_or_default();
    let to_ref: String   = dr.try_get("to_ref").unwrap_or_default();
    let service_name: String = dr.try_get("service_name").unwrap_or_else(|_| diff_id.clone());

    let consumer_name: Option<String> = if let Some(cid) = consumer_id {
        sqlx::query("SELECT name FROM consumer WHERE id = ?")
            .bind(cid)
            .fetch_optional(&pool)
            .await?
            .and_then(|r| r.try_get::<Option<String>, _>("name").ok().flatten())
    } else {
        None
    };

    let change_rows = sqlx::query(
        "SELECT path, kind, severity, description FROM change WHERE diff_id = ? AND severity = 'breaking' ORDER BY path",
    )
    .bind(&diff_id)
    .fetch_all(&pool)
    .await?;

    let evidence = load_diff_evidence(&pool, &diff_id, consumer_id).await?;

    let mut cs_q = String::from(
        "SELECT operation, field_path, file_path, line_number FROM call_site WHERE service_id IN (SELECT sv.service_id FROM diff d JOIN spec_version sv ON sv.id = d.from_version WHERE d.id = ?)"
    );
    if consumer_id.is_some() {
        cs_q.push_str(" AND consumer_id = ?");
    }
    cs_q.push_str(" ORDER BY operation, field_path LIMIT 100");
    let mut csqb = sqlx::query(&cs_q).bind(&diff_id);
    if let Some(cid) = consumer_id {
        csqb = csqb.bind(cid);
    }
    let call_sites = csqb.fetch_all(&pool).await?;

    let mut md = String::new();
    let scope_label = consumer_name.as_deref()
        .or(consumer_id)
        .map(|n| format!(" ��� scoped to **{n}**"))
        .unwrap_or_default();

    md.push_str(&format!(
        "# Migration Guide: {service_name}{scope_label}\n\n\
         **Diff:** `{from_ref}` → `{to_ref}`\n\n"
    ));

    if change_rows.is_empty() {
        md.push_str("No breaking changes in this diff.\n");
    } else {
        md.push_str(&format!(
            "## Breaking Changes ({})\n\n",
            change_rows.len()
        ));
        for cr in &change_rows {
            let path: String = cr.try_get("path").unwrap_or_default();
            let kind: String = cr.try_get("kind").unwrap_or_default();
            let desc: Option<String> = cr.try_get("description").unwrap_or(None);
            md.push_str(&format!("### `{path}` — {kind}\n\n"));
            if let Some(d) = desc {
                md.push_str(&format!("{d}\n\n"));
            }
            migration_advice(&mut md, &kind, &path);
        }
    }

    if !evidence.is_empty() {
        md.push_str(&format!("\n## Your Usage Evidence ({})\n\n", evidence.len()));
        md.push_str("The following operations and fields were observed from your service:\n\n");
        md.push_str("| Operation | Field | Source | Confidence | Last seen |\n");
        md.push_str("|---|---|---|---|---|\n");
        for ev in &evidence {
            let op  = ev["operation"].as_str().unwrap_or("—");
            let fp  = ev["field_path"].as_str().filter(|s| !s.is_empty()).unwrap_or("—");
            let src = ev["source_type"].as_str().unwrap_or("—");
            let conf = ev["confidence"].as_str().unwrap_or("—");
            let obs  = ev["observed_at"].as_str().map(|s| s.get(..10).unwrap_or(s)).unwrap_or("—");
            md.push_str(&format!("| `{op}` | `{fp}` | {src} | {conf} | {obs} |\n"));
        }
    }

    if !call_sites.is_empty() {
        md.push_str(&format!("\n## Call Sites ({})\n\n", call_sites.len()));
        md.push_str("Static references found in your codebase:\n\n");
        md.push_str("| Operation | Field | File | Line |\n");
        md.push_str("|---|---|---|---|\n");
        for cs in &call_sites {
            use sqlx::Row as _;
            let op:  String = cs.try_get("operation").unwrap_or_default();
            let fp:  String = cs.try_get("field_path").unwrap_or_default();
            let file: String = cs.try_get("file_path").unwrap_or_default();
            let line: Option<i64> = cs.try_get("line_number").unwrap_or(None);
            let line_str = line.map(|l| l.to_string()).unwrap_or_else(|| "—".into());
            md.push_str(&format!("| `{op}` | `{fp}` | `{file}` | {line_str} |\n"));
        }
    }

    Ok((
        [("Content-Type", "text/markdown; charset=utf-8")],
        md,
    ))
}

// POST /v1/diffs/:id/release-notes/generate
// Creates a release note row immediately (generation_status='pending') and
// spawns a background task to fill in the content.  Returns { id, generation_status }
// so the caller can poll GET /v1/release-notes/:id/generate-status.
pub(crate) async fn generate_release_note(
    Path(diff_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = caller_org(&org);
    require_org_owned(&pool, OrgResource::Diff, &diff_id, &org_id).await?;
    // Verify diff exists before queuing.
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM diff WHERE id = ?")
        .bind(&diff_id)
        .fetch_optional(&pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound(format!("diff {diff_id} not found")));
    }

    let id  = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO release_note (id, diff_id, content, generation_status, created_at) \
         VALUES (?, ?, '', 'pending', ?)",
    )
    .bind(&id)
    .bind(&diff_id)
    .bind(&now)
    .execute(&pool)
    .await?;

    // Audit start.
    {
        let p2 = pool.clone();
        let nid = id.clone();
        let did = diff_id.clone();
        let oid = org_id.clone();
        tokio::spawn(async move {
            crate::audit::record_event(&p2, &oid, "system", "release_note.generate.started",
                Some("release_note"), Some(&nid),
                Some(&serde_json::json!({ "diff_id": did }))).await;
        });
    }

    // Background generation task.
    {
        let p2  = pool.clone();
        let nid = id.clone();
        let did = diff_id.clone();
        let oid = org_id.clone();
        tokio::spawn(async move {
            match build_release_note_content(&p2, &did).await {
                Ok(md) => {
                    let _ = sqlx::query(
                        "UPDATE release_note SET content = ?, generation_status = 'completed' WHERE id = ?",
                    )
                    .bind(&md)
                    .bind(&nid)
                    .execute(&p2)
                    .await;
                    crate::audit::record_event(&p2, &oid, "system", "release_note.generate.completed",
                        Some("release_note"), Some(&nid), None).await;
                }
                Err(e) => {
                    let msg = e.to_string();
                    let _ = sqlx::query(
                        "UPDATE release_note SET generation_status = 'failed', generation_error = ? WHERE id = ?",
                    )
                    .bind(&msg)
                    .bind(&nid)
                    .execute(&p2)
                    .await;
                    crate::audit::record_event(&p2, &oid, "system", "release_note.generate.failed",
                        Some("release_note"), Some(&nid),
                        Some(&serde_json::json!({ "error": msg }))).await;
                }
            }
        });
    }

    Ok((
        axum::http::StatusCode::CREATED,
        axum::Json(serde_json::json!({
            "id":                id,
            "diff_id":           diff_id,
            "status":            "draft",
            "generation_status": "pending",
            "created_at":        now,
        })),
    ))
}

// GET /v1/release-notes/:id/generate-status
// Poll this endpoint after POST .../generate.  Returns generation_status and
// content once completed.
pub(crate) async fn get_generate_status(
    axum::extract::Path(id): axum::extract::Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: OrgExt,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    use sqlx::Row;

    require_org_owned(&pool, OrgResource::ReleaseNote, &id, &caller_org(&org)).await?;

    let row = sqlx::query(
        "SELECT id, diff_id, content, generation_status, generation_error, status, created_at \
         FROM release_note WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await?;

    let Some(r) = row else {
        return Err(ApiError::NotFound(format!("release note {id} not found")));
    };

    let gen_status: Option<String> = r.try_get("generation_status").ok().flatten();
    let gen_error:  Option<String> = r.try_get("generation_error").ok().flatten();
    let content:    String         = r.try_get("content").unwrap_or_default();
    let status:     String         = r.try_get("status").unwrap_or_else(|_| "draft".into());
    let created_at: String         = r.try_get("created_at").unwrap_or_default();

    let mut resp = serde_json::json!({
        "id":                id,
        "diff_id":           r.try_get::<String, _>("diff_id").unwrap_or_default(),
        "status":            status,
        "generation_status": gen_status,
        "created_at":        created_at,
    });

    if let Some(e) = gen_error {
        resp["generation_error"] = serde_json::json!(e);
    }
    // Only include content once generation is done — avoids returning an empty string.
    if resp["generation_status"] == "completed" {
        resp["content"] = serde_json::json!(content);
    }

    Ok(axum::Json(resp))
}

/// Build the Markdown content for a release note from DB.
/// Extracted so the background task and any future direct callers share the logic.
async fn build_release_note_content(
    pool: &sqlx::AnyPool,
    diff_id: &str,
) -> Result<String, crate::errors::ApiError> {
    use sqlx::Row;

    let diff_row = sqlx::query(
        r#"SELECT d.id, sv_from.git_ref AS from_ref, sv_to.git_ref AS to_ref,
                  s.name AS service_name
           FROM diff d
           JOIN spec_version sv_from ON sv_from.id = d.from_version
           JOIN spec_version sv_to   ON sv_to.id   = d.to_version
           JOIN service s            ON s.id        = sv_from.service_id
           WHERE d.id = ?"#,
    )
    .bind(diff_id)
    .fetch_optional(pool)
    .await?;

    let Some(dr) = diff_row else {
        return Err(ApiError::NotFound(format!("diff {diff_id} not found")));
    };

    let from_ref:     String = dr.try_get("from_ref").unwrap_or_default();
    let to_ref:       String = dr.try_get("to_ref").unwrap_or_default();
    let service_name: String = dr.try_get("service_name").unwrap_or_else(|_| diff_id.to_owned());

    let change_rows = sqlx::query(
        "SELECT path, kind, severity, description FROM change WHERE diff_id = ? ORDER BY severity DESC, path",
    )
    .bind(diff_id)
    .fetch_all(pool)
    .await?;

    let breaking: Vec<_> = change_rows.iter().filter(|r| r.try_get::<String, _>("severity").unwrap_or_default() == "breaking").collect();
    let risky: Vec<_>    = change_rows.iter().filter(|r| r.try_get::<String, _>("severity").unwrap_or_default() == "non_breaking_risky").collect();
    let safe: Vec<_>     = change_rows.iter().filter(|r| r.try_get::<String, _>("severity").unwrap_or_default() == "safe").collect();

    let mut md = String::new();
    md.push_str(&format!(
        "# Release Notes — {service_name}\n\n\
         **Versions:** `{from_ref}` → `{to_ref}`\n\n\
         **Summary:** {} breaking change{}, {} risky change{}, {} safe change{}\n\n",
        breaking.len(), if breaking.len() == 1 { "" } else { "s" },
        risky.len(),    if risky.len()    == 1 { "" } else { "s" },
        safe.len(),     if safe.len()     == 1 { "" } else { "s" },
    ));

    if !breaking.is_empty() {
        md.push_str(&format!("## Breaking Changes ({})\n\n", breaking.len()));
        md.push_str("> ⚠️ These changes will break consumers that depend on the affected fields or operations.\n\n");
        for row in &breaking {
            let path: String = row.try_get("path").unwrap_or_default();
            let kind: String = row.try_get("kind").unwrap_or_default();
            let desc: Option<String> = row.try_get("description").unwrap_or(None);
            md.push_str(&format!("### `{path}`\n\n**Kind:** `{kind}`\n\n"));
            if let Some(d) = desc { md.push_str(&format!("{d}\n\n")); }
            migration_advice(&mut md, &kind, &path);
        }
    }

    if !risky.is_empty() {
        md.push_str(&format!("## Risky Changes ({})\n\n", risky.len()));
        md.push_str("> ⚠️ These changes may affect some consumers. Review before deploying.\n\n");
        for row in &risky {
            let path: String = row.try_get("path").unwrap_or_default();
            let kind: String = row.try_get("kind").unwrap_or_default();
            md.push_str(&format!("- `{path}` — `{kind}`\n"));
        }
        md.push('\n');
    }

    if !safe.is_empty() {
        md.push_str(&format!("## Safe Changes ({})\n\n", safe.len()));
        for row in &safe {
            let path: String = row.try_get("path").unwrap_or_default();
            let kind: String = row.try_get("kind").unwrap_or_default();
            md.push_str(&format!("- `{path}` — `{kind}`\n"));
        }
        md.push('\n');
    }

    if change_rows.is_empty() {
        md.push_str("No changes detected in this diff.\n");
    }

    md.push_str("---\n\n*Generated by Radar Monitor*\n");
    Ok(md)
}

fn migration_advice(md: &mut String, kind: &str, path: &str) {
    let field = path.rsplit_once(" \u{2192} ").map(|(_, f)| f).unwrap_or(path);
    match kind {
        "field_removed" => {
            md.push_str(&format!(
                "**What changed:** `{field}` was removed from the response.\n\n\
                 **Action required:** Remove all code that reads `{field}`. \
                 Clients that access this field will receive `undefined` / `null`.\n\n"
            ));
        }
        "required_changed" => {
            md.push_str(&format!(
                "**What changed:** `{field}` is now required.\n\n\
                 **Action required:** Ensure your requests always include `{field}`. \
                 Omitting it will result in a 422 Unprocessable Entity response.\n\n"
            ));
        }
        "enum_value_removed" => {
            md.push_str(&format!(
                "**What changed:** A value was removed from the `{field}` enum.\n\n\
                 **Action required:** Audit your code for hardcoded uses of the removed value \
                 and replace with a currently supported value.\n\n"
            ));
        }
        "operation_removed" => {
            md.push_str(&format!(
                "**What changed:** The operation `{path}` was removed.\n\n\
                 **Action required:** Remove all calls to this operation. \
                 Check the release notes for a replacement endpoint.\n\n"
            ));
        }
        "type_changed" => {
            md.push_str(&format!(
                "**What changed:** The type of `{field}` changed.\n\n\
                 **Action required:** Update your request and response parsing to handle the new type. \
                 Sending the old type will result in a 422 response.\n\n"
            ));
        }
        "nullability_changed" => {
            md.push_str(&format!(
                "**What changed:** `{field}` is now non-nullable (or vice-versa).\n\n\
                 **Action required:** Guard all accesses to `{field}` with a null check, or \
                 update your serialisation code to handle the new nullability.\n\n"
            ));
        }
        _ => {
            md.push_str(&format!(
                "**What changed:** `{kind}` on `{field}`.\n\n\
                 **Action required:** Review the API changelog and update affected code paths.\n\n"
            ));
        }
    }
}
