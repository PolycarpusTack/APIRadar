// Audit event table — append-only via record_event(), read via GET /v1/audit-events.
// POST /v1/audit-events lets external callers (CLI, integrations) record events too.

use crate::auth::CallerOrg;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::AnyPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub(crate) struct ListQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    action: Option<String>,
    entity_type: Option<String>,
}

fn default_limit() -> i64 {
    100
}

#[derive(Deserialize)]
pub(crate) struct CreateBody {
    pub actor: String,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub meta: Option<Value>,
}

// Keys whose values must be redacted in audit event meta — prevents tokens,
// passwords, and API keys from appearing in the audit log.
const SECRET_KEYS: &[&str] = &[
    "token",
    "password",
    "secret",
    "key",
    "bearer",
    "api_key",
    "auth",
    "credential",
];

/// Redact any object field whose key contains a known secret keyword.
/// Applied recursively to nested objects.  Arrays and scalars are passed through.
fn redact_secrets(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                let lower = k.to_lowercase();
                if SECRET_KEYS.iter().any(|s| lower.contains(s)) {
                    out.insert(k.clone(), Value::String("[REDACTED]".into()));
                } else {
                    out.insert(k.clone(), redact_secrets(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_secrets).collect()),
        other => other.clone(),
    }
}

/// Internal helper — other modules call this to append an audit event.
/// Failures are silently swallowed so audit never blocks the caller.
pub(crate) async fn record_event(
    pool: &AnyPool,
    org_id: &str,
    actor: &str,
    action: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    meta: Option<&Value>,
) {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let meta_str = meta.map(|m| redact_secrets(m).to_string());
    let _ = q!("INSERT INTO audit_event \
         (id, org_id, actor, action, entity_type, entity_id, meta, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",)
    .bind(&id)
    .bind(org_id)
    .bind(actor)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(meta_str.as_deref())
    .bind(&now)
    .execute(pool)
    .await;
}

type RowTuple = (
    String,         // id
    String,         // org_id
    String,         // actor
    String,         // action
    Option<String>, // entity_type
    Option<String>, // entity_id
    Option<String>, // meta (JSON text)
    String,         // created_at
);

fn row_to_json(
    (id, org_id, actor, action, entity_type, entity_id, meta, created_at): RowTuple,
) -> Value {
    let meta_val: Value = meta
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    json!({
        "id": id,
        "org_id": org_id,
        "actor": actor,
        "action": action,
        "entity_type": entity_type,
        "entity_id": entity_id,
        "meta": meta_val,
        "created_at": created_at,
    })
}

pub(crate) async fn list_audit_events(
    State(pool): State<AnyPool>,
    caller: CallerOrg,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, StatusCode> {
    // Org isolation: read events scoped to the caller's org. Empty org
    // (desktop/no-auth) reads only the events written in that same empty-org
    // mode — consistent with create_audit_event / record_event, which now bind
    // the caller's org rather than a hardcoded "default".
    let org_id = caller.sql_scope().to_string();
    let org_id = org_id.as_str();

    let base = "SELECT id, org_id, actor, action, entity_type, entity_id, meta, created_at \
                FROM audit_event WHERE org_id = ?";

    let rows: Vec<RowTuple> = if let Some(action_filter) = &q.action {
        qa!(&format!(
            "{base} AND action LIKE ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
        ))
        .bind(org_id)
        .bind(format!("%{action_filter}%"))
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(&pool)
        .await
    } else if let Some(et) = &q.entity_type {
        qa!(&format!(
            "{base} AND entity_type = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
        ))
        .bind(org_id)
        .bind(et)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(&pool)
        .await
    } else {
        qa!(&format!("{base} ORDER BY created_at DESC LIMIT ? OFFSET ?"))
            .bind(org_id)
            .bind(q.limit)
            .bind(q.offset)
            .fetch_all(&pool)
            .await
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        json!({ "entries": rows.into_iter().map(row_to_json).collect::<Vec<_>>() }),
    ))
}

pub(crate) async fn create_audit_event(
    State(pool): State<AnyPool>,
    caller: CallerOrg,
    Json(body): Json<CreateBody>,
) -> (StatusCode, Json<Value>) {
    let org_id = caller.sql_scope().to_string();
    record_event(
        &pool,
        &org_id,
        &body.actor,
        &body.action,
        body.entity_type.as_deref(),
        body.entity_id.as_deref(),
        body.meta.as_ref(),
    )
    .await;
    (StatusCode::CREATED, Json(json!({ "ok": true })))
}
