use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::auth::JwtClaims;
use crate::errors::ApiError;
use crate::utils::{is_host_allowed, is_ssrf_blocked};

// ---------------------------------------------------------------------------
// HMAC-SHA256 payload signing (ADR-K-2)
// ---------------------------------------------------------------------------

pub(crate) fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct CreateWebhookBody {
    url: String,
    #[serde(default = "default_events")]
    events: Vec<String>,
    #[serde(default = "default_webhook_type")]
    #[serde(rename = "type")]
    webhook_type: String,
}

fn default_webhook_type() -> String { "generic".to_string() }

fn default_events() -> Vec<String> {
    vec!["diff.created".to_string()]
}

#[derive(Serialize)]
struct WebhookResponse {
    id: String,
    org_id: String,
    url: String,
    events: Vec<String>,
    #[serde(rename = "type")]
    webhook_type: String,
    secret: Option<String>,
    secret_hint: String,
    active: bool,
    created_at: String,
}

fn mask_secret(secret: &str, reveal: bool) -> (Option<String>, String) {
    // Use char_indices to avoid panicking on multi-byte UTF-8 characters.
    let boundary = secret.char_indices().nth(4).map(|(i, _)| i).unwrap_or(secret.len());
    let hint = format!("{}****", &secret[..boundary]);
    if reveal {
        (Some(secret.to_string()), hint)
    } else {
        (None, hint)
    }
}

fn row_to_response(row: &sqlx::any::AnyRow, reveal_secret: bool) -> WebhookResponse {
    let secret: String = row.get("secret");
    let (sec_full, hint) = mask_secret(&secret, reveal_secret);
    let events_str: String = row.get("events");
    WebhookResponse {
        id: row.get("id"),
        org_id: row.get("org_id"),
        url: row.get("url"),
        events: events_str.split(',').map(|s| s.trim().to_string()).collect(),
        webhook_type: row.try_get("type").unwrap_or_else(|_| "generic".to_string()),
        secret: sec_full,
        secret_hint: hint,
        active: {
            let v: i32 = row.get("active");
            v != 0
        },
        created_at: row.get("created_at"),
    }
}

// ---------------------------------------------------------------------------
// POST /v1/webhooks
// ---------------------------------------------------------------------------

pub(crate) async fn create_webhook(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateWebhookBody>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    if is_ssrf_blocked(&body.url) || !is_host_allowed(&body.url) {
        return Err(ApiError::BadRequest(
            "url must be a reachable HTTPS endpoint outside private address space".into(),
        ));
    }

    let events_str = body.events.join(",");
    let secret = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Idempotent on (org_id, url, events)
    let existing = sqlx::query(
        "SELECT id FROM webhook WHERE org_id = ? AND url = ? AND events = ?",
    )
    .bind(&org_id)
    .bind(&body.url)
    .bind(&events_str)
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        let id: String = row.get("id");
        let wh = sqlx::query(
            "SELECT id, org_id, url, events, secret, active, created_at, type FROM webhook WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await?;
        return Ok((StatusCode::OK, Json(row_to_response(&wh, false))));
    }

    let webhook_type = if ["generic", "slack"].contains(&body.webhook_type.as_str()) {
        body.webhook_type.clone()
    } else {
        "generic".to_string()
    };

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO webhook (id, org_id, url, events, secret, active, created_at, type) VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&id)
    .bind(&org_id)
    .bind(&body.url)
    .bind(&events_str)
    .bind(&secret)
    .bind(&now)
    .bind(&webhook_type)
    .execute(&pool)
    .await?;

    let wh = sqlx::query(
        "SELECT id, org_id, url, events, secret, active, created_at, type FROM webhook WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await?;

    Ok((StatusCode::CREATED, Json(row_to_response(&wh, true))))
}

// ---------------------------------------------------------------------------
// GET /v1/webhooks
// ---------------------------------------------------------------------------

pub(crate) async fn list_webhooks(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let rows = sqlx::query(
        "SELECT id, org_id, url, events, secret, active, created_at, type FROM webhook WHERE org_id = ? ORDER BY created_at DESC",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await?;

    let list: Vec<WebhookResponse> = rows.iter().map(|r| row_to_response(r, false)).collect();
    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// DELETE /v1/webhooks/:id
// ---------------------------------------------------------------------------

pub(crate) async fn delete_webhook(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let row = sqlx::query("SELECT id FROM webhook WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .fetch_optional(&pool)
        .await?;

    if row.is_none() {
        return Err(ApiError::NotFound(format!("webhook {id} not found")));
    }

    sqlx::query("DELETE FROM webhook WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .execute(&pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /v1/webhooks/:id/test — fire a synthetic "ping" event
// ---------------------------------------------------------------------------

pub(crate) async fn test_webhook(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let row = sqlx::query(
        "SELECT id, url, secret, type FROM webhook WHERE id = ? AND org_id = ? AND active = 1",
    )
    .bind(&id)
    .bind(&org_id)
    .fetch_optional(&pool)
    .await?;

    let row = row.ok_or_else(|| ApiError::NotFound(format!("webhook {id} not found or inactive")))?;
    let url: String = row.get("url");
    let secret: String = row.get("secret");
    let webhook_type: String = row.try_get("type").unwrap_or_else(|_| "generic".to_string());

    let payload = if webhook_type == "slack" {
        json!({
            "blocks": [{
                "type": "section",
                "text": { "type": "mrkdwn", "text": "⚡ *API Radar* test ping — your webhook is working!" }
            }]
        })
    } else {
        json!({
            "event": "ping",
            "webhook_id": id,
            "message": "API Radar test ping"
        })
    };

    tokio::spawn(deliver_webhook_event(DeliveryTask {
        pool,
        org_id,
        webhook_id: id,
        url,
        secret,
        webhook_type,
        event: "ping".to_string(),
        payload,
    }));

    Ok((StatusCode::ACCEPTED, Json(json!({"status": "ping dispatched"}))))
}

// ---------------------------------------------------------------------------
// Delivery engine (K-1-T3)
// ---------------------------------------------------------------------------

/// Called after any diff is persisted to fire `diff.created` events.
pub(crate) async fn dispatch_diff_event(pool: sqlx::AnyPool, diff_id: String, org_id: String) {
    let payload = match build_diff_payload(&pool, &diff_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("webhook dispatch: failed to build payload for diff {diff_id}: {e}");
            return;
        }
    };

    let rows = match sqlx::query(
        "SELECT id, url, secret, type FROM webhook WHERE org_id = ? AND active = 1 AND (events = 'diff.created' OR events LIKE '%diff.created%')",
    )
    .bind(&org_id)
    .fetch_all(&pool)
    .await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("webhook dispatch: db query failed: {e}");
            return;
        }
    };

    for row in rows {
        let wh_id: String = row.get("id");
        let url: String = row.get("url");
        let secret: String = row.get("secret");
        let wh_type: String = row.try_get("type").unwrap_or_else(|_| "generic".to_string());

        let send_payload = if wh_type == "slack" {
            build_slack_block_kit(&payload)
        } else {
            payload.clone()
        };

        tokio::spawn(deliver_webhook_event(DeliveryTask {
            pool: pool.clone(),
            org_id: org_id.clone(),
            webhook_id: wh_id,
            url,
            secret,
            webhook_type: wh_type,
            event: "diff.created".to_string(),
            payload: send_payload,
        }));
    }
}

async fn build_diff_payload(pool: &sqlx::AnyPool, diff_id: &str) -> anyhow::Result<serde_json::Value> {
    // diff has no service_id or change counts directly — join through spec_version → service
    // and aggregate change counts from the change table (portable SQL, no dialect-specific syntax).
    let row = sqlx::query(
        "SELECT d.id, d.created_at, sv.service_id, s.name as service_name,
                COALESCE(SUM(CASE WHEN c.severity = 'breaking' THEN 1 ELSE 0 END), 0) as breaking_change_count,
                COUNT(c.id) as total_change_count
         FROM diff d
         JOIN spec_version sv ON sv.id = d.to_version
         JOIN service s ON s.id = sv.service_id
         LEFT JOIN change c ON c.diff_id = d.id
         WHERE d.id = ?
         GROUP BY d.id, d.created_at, sv.service_id, s.name",
    )
    .bind(diff_id)
    .fetch_one(pool)
    .await?;

    Ok(json!({
        "event": "diff.created",
        "diff_id": row.get::<String, _>("id"),
        "service_id": row.get::<String, _>("service_id"),
        "service_name": row.get::<String, _>("service_name"),
        "breaking_count": row.get::<i64, _>("breaking_change_count"),
        "changes_count": row.get::<i64, _>("total_change_count"),
        "created_at": row.get::<String, _>("created_at"),
    }))
}

fn build_slack_block_kit(diff_payload: &serde_json::Value) -> serde_json::Value {
    let service_name = diff_payload.get("service_name").and_then(|v| v.as_str()).unwrap_or("unknown");
    let breaking = diff_payload.get("breaking_count").and_then(|v| v.as_i64()).unwrap_or(0);
    let total = diff_payload.get("changes_count").and_then(|v| v.as_i64()).unwrap_or(0);
    let diff_id = diff_payload.get("diff_id").and_then(|v| v.as_str()).unwrap_or("");

    json!({
        "blocks": [
            {
                "type": "header",
                "text": { "type": "plain_text", "text": format!("⚡ API Drift Detected — {service_name}") }
            },
            {
                "type": "section",
                "fields": [
                    { "type": "mrkdwn", "text": format!("*Breaking Changes*\n{breaking}") },
                    { "type": "mrkdwn", "text": format!("*Total Changes*\n{total}") }
                ]
            },
            {
                "type": "actions",
                "elements": [{
                    "type": "button",
                    "text": { "type": "plain_text", "text": "View Diff" },
                    "url": format!("/diffs/{diff_id}")
                }]
            }
        ]
    })
}

struct DeliveryTask {
    pool: sqlx::AnyPool,
    org_id: String,
    webhook_id: String,
    url: String,
    secret: String,
    webhook_type: String,
    event: String,
    payload: serde_json::Value,
}

async fn deliver_webhook_event(t: DeliveryTask) {
    let DeliveryTask { pool, org_id, webhook_id, url, secret, webhook_type, event, payload } = t;
    let delivery_id = Uuid::new_v4().to_string();
    let body = serde_json::to_string(&payload).unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    // Insert pending delivery record
    let _ = sqlx::query(
        "INSERT INTO webhook_delivery (id, webhook_id, event, payload, status, attempt, error, delivered_at) VALUES (?, ?, ?, ?, 'pending', 0, NULL, NULL)",
    )
    .bind(&delivery_id)
    .bind(&webhook_id)
    .bind(&event)
    .bind(&body)
    .execute(&pool)
    .await;

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("radar-api/webhook")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default();

    // Retry with exponential backoff: 1s, 4s, 16s
    let delays = [0u64, 1, 4, 16];
    let mut last_error: Option<String> = None;

    for (attempt, delay) in delays.iter().enumerate() {
        if *delay > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
        }

        let mut req_builder = http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Radar-Event", &event)
            .body(body.clone());

        if webhook_type != "slack" {
            let signature = sign_payload(&secret, body.as_bytes());
            req_builder = req_builder.header("X-Radar-Signature-256", signature);
        }

        match req_builder.send().await
        {
            Ok(resp) if resp.status().is_success() => {
                let _ = sqlx::query(
                    "UPDATE webhook_delivery SET status = 'delivered', attempt = ?, delivered_at = ? WHERE id = ?",
                )
                .bind(attempt as i32 + 1)
                .bind(&now)
                .bind(&delivery_id)
                .execute(&pool)
                .await;
                crate::audit::record_event(&pool, &org_id, "system", "webhook.delivered", Some("webhook"), Some(&webhook_id), Some(&serde_json::json!({ "event": event, "delivery_id": delivery_id }))).await;
                return;
            }
            Ok(resp) => {
                last_error = Some(format!("HTTP {}", resp.status()));
            }
            Err(e) => {
                last_error = Some(e.to_string());
            }
        }

        let _ = sqlx::query(
            "UPDATE webhook_delivery SET attempt = ?, error = ? WHERE id = ?",
        )
        .bind(attempt as i32 + 1)
        .bind(last_error.as_deref())
        .bind(&delivery_id)
        .execute(&pool)
        .await;
    }

    // All attempts exhausted
    let _ = sqlx::query(
        "UPDATE webhook_delivery SET status = 'failed', error = ? WHERE id = ?",
    )
    .bind(last_error.as_deref())
    .bind(&delivery_id)
    .execute(&pool)
    .await;
    crate::audit::record_event(&pool, &org_id, "system", "webhook.failed", Some("webhook"), Some(&webhook_id), Some(&serde_json::json!({ "event": event, "error": last_error }))).await;
}

// ---------------------------------------------------------------------------
// GET /v1/webhooks/:id/deliveries — delivery audit log
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_payload_starts_with_sha256_prefix() {
        let sig = sign_payload("secret", b"hello world");
        assert!(sig.starts_with("sha256="), "expected sha256= prefix, got: {sig}");
        assert_eq!(sig.len(), 7 + 64, "expected sha256= + 64 hex chars");
    }

    #[test]
    fn sign_payload_is_deterministic() {
        assert_eq!(sign_payload("key", b"body"), sign_payload("key", b"body"));
    }

    #[test]
    fn sign_payload_changes_with_different_secret() {
        assert_ne!(sign_payload("secret1", b"body"), sign_payload("secret2", b"body"));
    }

    #[test]
    fn sign_payload_changes_with_different_body() {
        assert_ne!(sign_payload("key", b"body-a"), sign_payload("key", b"body-b"));
    }

    #[test]
    fn mask_secret_shows_first_4_ascii_chars() {
        let (_, hint) = mask_secret("abcdefgh", false);
        assert_eq!(hint, "abcd****");
    }

    #[test]
    fn mask_secret_handles_short_secret() {
        let (_, hint) = mask_secret("ab", false);
        assert_eq!(hint, "ab****");
    }

    #[test]
    fn mask_secret_handles_multibyte_utf8_without_panic() {
        // "€" is 3 bytes; slicing at byte 4 would panic without char_indices
        let (_, hint) = mask_secret("€€€€€", false);
        assert!(hint.ends_with("****"));
    }

    #[test]
    fn mask_secret_reveals_full_when_requested() {
        let (full, _) = mask_secret("my-secret", true);
        assert_eq!(full, Some("my-secret".to_string()));
    }

    #[test]
    fn mask_secret_hides_full_when_not_requested() {
        let (full, _) = mask_secret("my-secret", false);
        assert_eq!(full, None);
    }
}

pub(crate) async fn list_deliveries(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    // Verify webhook belongs to caller's org
    let row = sqlx::query("SELECT id FROM webhook WHERE id = ? AND org_id = ?")
        .bind(&id)
        .bind(&org_id)
        .fetch_optional(&pool)
        .await?;
    if row.is_none() {
        return Err(ApiError::NotFound(format!("webhook {id} not found")));
    }

    let rows = sqlx::query(
        "SELECT id, webhook_id, event, status, attempt, error, delivered_at FROM webhook_delivery WHERE webhook_id = ? ORDER BY rowid DESC LIMIT 50",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await?;

    let list: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<String, _>("id"),
                "webhook_id": r.get::<String, _>("webhook_id"),
                "event": r.get::<String, _>("event"),
                "status": r.get::<String, _>("status"),
                "attempt": r.get::<i32, _>("attempt"),
                "error": r.try_get::<String, _>("error").ok(),
                "delivered_at": r.try_get::<String, _>("delivered_at").ok(),
            })
        })
        .collect();

    Ok(Json(list))
}

// ---------------------------------------------------------------------------
// TD-K5: Startup outbox sweep — re-dispatch deliveries abandoned mid-flight
// ---------------------------------------------------------------------------

pub(crate) fn start_webhook_outbox(pool: sqlx::AnyPool) {
    tokio::spawn(async move {
        // Wait for the server to finish binding/routing before issuing DB queries.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let rows = match sqlx::query(
            "SELECT wd.id, wd.event, wd.payload, \
                    w.id as webhook_id, w.url, w.secret, w.type as webhook_type \
             FROM webhook_delivery wd \
             JOIN webhook w ON w.id = wd.webhook_id \
             WHERE wd.status = 'pending'",
        )
        .fetch_all(&pool)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("outbox: failed to load pending deliveries: {e}");
                return;
            }
        };

        if rows.is_empty() {
            return;
        }

        tracing::info!("outbox: re-dispatching {} pending delivery/deliveries", rows.len());

        for row in rows {
            let delivery_id: String = row.get("id");
            let event: String = row.get("event");
            let payload_str: String = row.get("payload");
            let url: String = row.get("url");
            let secret: String = row.get("secret");
            let webhook_type: String = row.get("webhook_type");

            let payload: serde_json::Value =
                serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

            let pool2 = pool.clone();
            tokio::spawn(async move {
                retry_pending_delivery(pool2, delivery_id, url, secret, webhook_type, event, payload).await;
            });
        }
    });
}

/// Re-attempt a delivery left `pending` after a crash, updating the existing record.
async fn retry_pending_delivery(
    pool: sqlx::AnyPool,
    delivery_id: String,
    url: String,
    secret: String,
    webhook_type: String,
    event: String,
    payload: serde_json::Value,
) {
    let body = serde_json::to_string(&payload).unwrap_or_default();
    let now = Utc::now().to_rfc3339();

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("radar-api/webhook-outbox")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default();

    let delays = [0u64, 1, 4, 16];
    let mut last_error: Option<String> = None;

    for (attempt, delay) in delays.iter().enumerate() {
        if *delay > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
        }

        let mut req_builder = http
            .post(&url)
            .header("Content-Type", "application/json")
            .header("X-Radar-Event", &event)
            .body(body.clone());

        if webhook_type != "slack" {
            let signature = sign_payload(&secret, body.as_bytes());
            req_builder = req_builder.header("X-Radar-Signature-256", signature);
        }

        match req_builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let _ = sqlx::query(
                    "UPDATE webhook_delivery SET status = 'delivered', attempt = ?, delivered_at = ? WHERE id = ?",
                )
                .bind(attempt as i32 + 1)
                .bind(&now)
                .bind(&delivery_id)
                .execute(&pool)
                .await;
                tracing::info!(
                    "outbox: delivery {delivery_id} succeeded on attempt {}",
                    attempt + 1
                );
                return;
            }
            Ok(resp) => {
                last_error = Some(format!("HTTP {}", resp.status()));
            }
            Err(e) => {
                last_error = Some(e.to_string());
            }
        }

        let _ = sqlx::query(
            "UPDATE webhook_delivery SET attempt = ?, error = ? WHERE id = ?",
        )
        .bind(attempt as i32 + 1)
        .bind(last_error.as_deref())
        .bind(&delivery_id)
        .execute(&pool)
        .await;
    }

    let _ = sqlx::query(
        "UPDATE webhook_delivery SET status = 'failed', error = ? WHERE id = ?",
    )
    .bind(last_error.as_deref())
    .bind(&delivery_id)
    .execute(&pool)
    .await;
    tracing::warn!(
        "outbox: delivery {delivery_id} permanently failed: {:?}",
        last_error
    );
}
