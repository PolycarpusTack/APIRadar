use axum::{extract::State, http::StatusCode, response::IntoResponse};
use chrono::{Datelike, Timelike, Utc};
use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde_json::json;
use sqlx::Row;

use crate::errors::ApiError;

// ---------------------------------------------------------------------------
// SMTP configuration from environment
// ---------------------------------------------------------------------------

struct SmtpConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    from: String,
    recipients: Vec<String>,
}

fn smtp_config() -> Option<SmtpConfig> {
    let host = std::env::var("RADAR_SMTP_HOST").ok().filter(|s| !s.is_empty())?;
    let port: u16 = std::env::var("RADAR_SMTP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(587);
    let user = std::env::var("RADAR_SMTP_USER").unwrap_or_default();
    let password = std::env::var("RADAR_SMTP_PASSWORD").unwrap_or_default();
    let from = std::env::var("RADAR_SMTP_FROM")
        .unwrap_or_else(|_| format!("noreply@{host}"));
    let recipients_raw = std::env::var("RADAR_DIGEST_RECIPIENTS").unwrap_or_default();
    let recipients: Vec<String> = recipients_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if recipients.is_empty() {
        return None;
    }

    Some(SmtpConfig { host, port, user, password, from, recipients })
}

// ---------------------------------------------------------------------------
// Digest aggregation
// ---------------------------------------------------------------------------

pub(crate) struct DigestData {
    pub(crate) total_diffs: i64,
    pub(crate) breaking_diffs: i64,
    pub(crate) top_services: Vec<(String, i64)>,
}

/// Aggregate the weekly digest. Pass an empty `org_id` for the global digest
/// (scheduled send); a non-empty `org_id` scopes every count to that org via the
/// `(? = '' OR ...)` no-op guard. The org value is bound twice because sqlx
/// `Any` uses positional `?` placeholders.
async fn aggregate_digest(pool: &sqlx::AnyPool, org_id: &str) -> anyhow::Result<DigestData> {
    let window_start = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();

    let total_row = sqlx::query(
        "SELECT COUNT(*) as cnt FROM diff d \
         JOIN spec_version sv ON sv.id = d.to_version \
         JOIN service s ON s.id = sv.service_id \
         WHERE d.created_at >= ? AND (? = '' OR s.org_id = ?)",
    )
    .bind(&window_start)
    .bind(org_id)
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    let total_diffs: i64 = total_row.try_get("cnt").unwrap_or(0);

    let breaking_row = sqlx::query(
        "SELECT COUNT(DISTINCT d.id) as cnt FROM diff d \
         JOIN change c ON c.diff_id = d.id \
         JOIN spec_version sv ON sv.id = d.to_version \
         JOIN service s ON s.id = sv.service_id \
         WHERE c.severity = 'breaking' AND d.created_at >= ? AND (? = '' OR s.org_id = ?)",
    )
    .bind(&window_start)
    .bind(org_id)
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    let breaking_diffs: i64 = breaking_row.try_get("cnt").unwrap_or(0);

    let service_rows = sqlx::query(
        "SELECT s.name, COUNT(d.id) as diff_count \
         FROM diff d \
         JOIN spec_version sv ON sv.id = d.to_version \
         JOIN service s ON s.id = sv.service_id \
         WHERE d.created_at >= ? AND (? = '' OR s.org_id = ?) \
         GROUP BY s.id, s.name \
         ORDER BY diff_count DESC \
         LIMIT 3",
    )
    .bind(&window_start)
    .bind(org_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let top_services = service_rows
        .iter()
        .map(|r| {
            (
                r.try_get::<String, _>("name").unwrap_or_default(),
                r.try_get::<i64, _>("diff_count").unwrap_or(0),
            )
        })
        .collect();

    Ok(DigestData { total_diffs, breaking_diffs, top_services })
}

// ---------------------------------------------------------------------------
// HTML template render
// ---------------------------------------------------------------------------

pub(crate) fn render_digest_html(data: &DigestData) -> String {
    let service_rows: String = data
        .top_services
        .iter()
        .map(|(name, count)| {
            format!(
                "<tr><td style='padding:6px 12px;border-bottom:1px solid #e5e7eb'>{name}</td>\
                 <td style='padding:6px 12px;border-bottom:1px solid #e5e7eb;text-align:right'>{count}</td></tr>"
            )
        })
        .collect::<Vec<_>>()
        .join("");

    let top_section = if data.top_services.is_empty() {
        String::new()
    } else {
        format!(
            "<h2 style='font-size:14px;font-weight:600;margin-bottom:8px'>Top services by drift activity</h2>\
             <table style='width:100%;border-collapse:collapse;border:1px solid #e5e7eb;border-radius:6px;overflow:hidden'>\
             {service_rows}\
             </table>"
        )
    };

    format!(
        "<!DOCTYPE html>\
         <html><head><meta charset='utf-8'><title>API Radar Weekly Digest</title></head>\
         <body style='font-family:system-ui,sans-serif;max-width:560px;margin:40px auto;color:#111827'>\
         <h1 style='font-size:22px;margin-bottom:4px'>API Radar &mdash; Weekly Digest</h1>\
         <p style='color:#6b7280;font-size:13px;margin-top:0'>Past 7 days</p>\
         <table style='width:100%;border-collapse:collapse;margin:20px 0;background:#f9fafb;border:1px solid #e5e7eb;border-radius:8px;overflow:hidden'>\
         <tr>\
           <td style='padding:16px 20px'>\
             <div style='font-size:28px;font-weight:700;color:#1d4ed8'>{total_diffs}</div>\
             <div style='font-size:12px;color:#6b7280'>total diffs</div>\
           </td>\
           <td style='padding:16px 20px;border-left:1px solid #e5e7eb'>\
             <div style='font-size:28px;font-weight:700;color:#dc2626'>{breaking_diffs}</div>\
             <div style='font-size:12px;color:#6b7280'>diffs with breaking changes</div>\
           </td>\
         </tr>\
         </table>\
         {top_section}\
         <p style='font-size:12px;color:#9ca3af;margin-top:32px'>Generated by API Radar</p>\
         </body></html>",
        total_diffs = data.total_diffs,
        breaking_diffs = data.breaking_diffs,
        top_section = top_section,
    )
}

// ---------------------------------------------------------------------------
// POST /v1/notifications/digest/preview (K-5)
// ---------------------------------------------------------------------------

pub(crate) async fn preview_digest(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<crate::auth::JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let data = aggregate_digest(&pool, &org_id).await.map_err(|_| {
        ApiError::BadRequest("failed to aggregate digest data".into())
    })?;

    let html = render_digest_html(&data);

    Ok((
        StatusCode::OK,
        axum::response::Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .body(axum::body::Body::from(html))
            .unwrap(),
    ))
}

// ---------------------------------------------------------------------------
// Weekly digest background task
// ---------------------------------------------------------------------------

pub(crate) fn start_digest_scheduler(pool: sqlx::AnyPool) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let now = Utc::now();
            if now.weekday() == chrono::Weekday::Mon && now.hour() == 8 {
                // ISO year-week key prevents duplicate sends across restarts.
                let week_key = format!("digest_sent_{}", now.format("%G-W%V"));
                let already_sent = sqlx::query(
                    "SELECT 1 FROM settings WHERE key = ?",
                )
                .bind(&week_key)
                .fetch_optional(&pool)
                .await
                .map(|r| r.is_some())
                .unwrap_or(false);

                if already_sent {
                    tracing::info!("digest: already sent for {week_key}, skipping");
                    continue;
                }

                send_digest(&pool).await;

                let _ = sqlx::query(
                    "INSERT INTO settings (key, value) VALUES (?, ?)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                )
                .bind(&week_key)
                .bind(Utc::now().to_rfc3339())
                .execute(&pool)
                .await;
            }
        }
    });
}

async fn send_digest(pool: &sqlx::AnyPool) {
    let config = match smtp_config() {
        Some(c) => c,
        None => {
            tracing::info!("digest: SMTP not configured, skipping");
            return;
        }
    };

    // Scheduled global send: empty org_id aggregates across all orgs.
    let data = match aggregate_digest(pool, "").await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("digest: aggregation failed: {e}");
            return;
        }
    };

    let html = render_digest_html(&data);

    let mailer = match AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host) {
        Ok(b) => b
            .port(config.port)
            .credentials(Credentials::new(config.user.clone(), config.password.clone()))
            .build(),
        Err(e) => {
            tracing::warn!("digest: SMTP relay setup failed: {e}");
            return;
        }
    };

    for recipient in &config.recipients {
        let msg = match Message::builder()
            .from(
                config
                    .from
                    .parse()
                    .unwrap_or_else(|_| "noreply@radar".parse().unwrap()),
            )
            .to(match recipient.parse() {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!("digest: invalid recipient address: {recipient}");
                    continue;
                }
            })
            .subject("API Radar — Weekly Digest")
            .header(ContentType::TEXT_HTML)
            .body(html.clone())
        {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("digest: failed to build message for {recipient}: {e}");
                continue;
            }
        };

        if let Err(e) = mailer.send(msg).await {
            tracing::warn!("digest: SMTP send to {recipient} failed: {e}");
        } else {
            tracing::info!("digest: sent to {recipient}");
        }
    }
}

/// Parse a GitHub PR URL and post a 'success' status check.
pub(crate) async fn post_github_status_acknowledged(pr_url: &str) {
    let token = match std::env::var("GITHUB_TOKEN").ok().filter(|s| !s.is_empty()) {
        Some(t) => t,
        None => return,
    };

    // Parse owner/repo/pull_number: https://github.com/owner/repo/pull/123
    let parts: Vec<&str> = pr_url.trim_end_matches('/').split('/').collect();
    if parts.len() < 7 || parts[5] != "pull" {
        tracing::warn!("github_status: could not parse PR URL: {pr_url}");
        return;
    }
    let owner = parts[3];
    let repo = parts[4];
    let pull_number = parts[6];

    let http = match reqwest::Client::builder()
        .user_agent("radar-api/github-status")
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    // Fetch PR head SHA
    let sha = match http
        .get(format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{pull_number}"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(v) => v["head"]["sha"].as_str().map(|s| s.to_string()),
            Err(_) => None,
        },
        _ => None,
    };

    let sha = match sha {
        Some(s) => s,
        None => {
            tracing::warn!("github_status: could not fetch PR SHA for {pr_url}");
            return;
        }
    };

    let status_body = json!({
        "state": "success",
        "description": "Acknowledged in API Radar",
        "context": "api-radar/drift-check"
    });

    match http
        .post(format!(
            "https://api.github.com/repos/{owner}/{repo}/statuses/{sha}"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .json(&status_body)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            tracing::info!("github_status: posted 'success' for {pr_url}");
        }
        Ok(r) => tracing::warn!("github_status: POST failed with HTTP {}", r.status()),
        Err(e) => tracing::warn!("github_status: request error: {e}"),
    }
}

// ---------------------------------------------------------------------------
// K-6: GitHub status check on acknowledgement
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data(services: Vec<(&str, i64)>) -> DigestData {
        DigestData {
            total_diffs: 42,
            breaking_diffs: 7,
            top_services: services.into_iter().map(|(n, c)| (n.to_string(), c)).collect(),
        }
    }

    #[test]
    fn render_digest_html_is_valid_html_document() {
        let html = render_digest_html(&sample_data(vec![]));
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn render_digest_html_contains_total_diffs() {
        let html = render_digest_html(&sample_data(vec![]));
        assert!(html.contains("42"), "total_diffs 42 must appear in output");
    }

    #[test]
    fn render_digest_html_contains_breaking_diffs() {
        let html = render_digest_html(&sample_data(vec![]));
        assert!(html.contains("7"), "breaking_diffs 7 must appear in output");
    }

    #[test]
    fn render_digest_html_contains_service_names_when_present() {
        let data = sample_data(vec![("payments-api", 5), ("billing-svc", 3)]);
        let html = render_digest_html(&data);
        assert!(html.contains("payments-api"));
        assert!(html.contains("billing-svc"));
        assert!(html.contains("5"));
        assert!(html.contains("3"));
    }

    #[test]
    fn render_digest_html_omits_table_when_no_services() {
        let data = sample_data(vec![]);
        let html = render_digest_html(&data);
        assert!(!html.contains("Top services"));
    }

    #[test]
    fn render_digest_html_contains_branding() {
        let html = render_digest_html(&sample_data(vec![]));
        assert!(html.contains("API Radar"));
        assert!(html.contains("Weekly Digest"));
    }
}
