use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::auth::{JwtClaims, assert_org_access};
use crate::errors::ApiError;
use crate::utils::collection_evidence_id;

#[derive(serde::Deserialize)]
pub(crate) struct CreateConsumerBody {
    pub(crate) name: String,
    pub(crate) repo_url: String,
    pub(crate) owner_team: String,
    pub(crate) contact: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct UpsertConsumerByNameBody {
    pub(crate) name: String,
    #[serde(default = "default_catalog_source")]
    pub(crate) catalog_source: String,
}

fn default_catalog_source() -> String {
    "collection_file".to_string()
}

#[derive(serde::Deserialize)]
pub(crate) struct CollectionEvidenceItem {
    pub(crate) consumer_id: String,
    pub(crate) service_id: String,
    #[serde(default)]
    pub(crate) operation: String,
    #[serde(default)]
    pub(crate) field_path: String,
    #[serde(default)]
    pub(crate) evidence_uri: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct CreateSubscriptionBody {
    pub(crate) consumer_id: String,
}

// GET /v1/services/:id/consumers
pub(crate) async fn list_consumers(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    // Org isolation: authenticated callers may only list consumers of their own services.
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
        SELECT c.id, c.name, c.repo_url, c.owner_team, c.contact
        FROM consumer c
        JOIN subscription s ON s.consumer_id = c.id
        WHERE s.service_id = ?
        "#,
    )
    .bind(&service_id)
    .fetch_all(&pool)
    .await?;

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":         row.get::<String, _>("id"),
                "name":       row.get::<String, _>("name"),
                "repo_url":   row.get::<String, _>("repo_url"),
                "owner_team": row.get::<String, _>("owner_team"),
                "contact":    row.get::<String, _>("contact"),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}

// POST /v1/consumers
pub(crate) async fn create_consumer(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<CreateConsumerBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO consumer (id, name, repo_url, owner_team, contact, org_id)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(&id)
    .bind(&body.name)
    .bind(&body.repo_url)
    .bind(&body.owner_team)
    .bind(&body.contact)
    .bind(&org_id)
    .execute(&pool)
    .await?;

    metrics::counter!("radar_consumers_created_total").increment(1);

    {
        let pool2 = pool.clone();
        let oid = org_id.clone();
        let cid = id.clone();
        tokio::spawn(async move {
            crate::audit::record_event(&pool2, &oid, "system", "consumer.registered", Some("consumer"), Some(&cid), None).await;
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":   id,
            "name": body.name,
        })),
    ))
}

// POST /v1/consumers/upsert — auto-register a consumer by name (idempotent on org_id+name).
pub(crate) async fn upsert_consumer_by_name(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(body): Json<UpsertConsumerByNameBody>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    // Atomic upsert: INSERT ... ON CONFLICT (org_id, name) DO NOTHING, then
    // fetch back the (possibly pre-existing) row. This avoids the TOCTOU race
    // of SELECT-then-INSERT under concurrent scanner runs.
    // Requires the UNIQUE index on (org_id, name) from migration 020.
    let new_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO consumer (id, name, repo_url, owner_team, contact, org_id, catalog_source) \
         VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (org_id, name) DO NOTHING",
    )
    .bind(&new_id)
    .bind(&name)
    .bind("")
    .bind("")
    .bind("")
    .bind(&org_id)
    .bind(&body.catalog_source)
    .execute(&pool)
    .await?;

    let row = sqlx::query(
        "SELECT id FROM consumer WHERE org_id = ? AND name = ? LIMIT 1",
    )
    .bind(&org_id)
    .bind(&name)
    .fetch_one(&pool)
    .await?;
    let id: String = row.try_get("id").unwrap_or(new_id.clone());
    let created = id == new_id;

    Ok((
        if created { StatusCode::CREATED } else { StatusCode::OK },
        Json(json!({"id": id, "name": name, "created": created})),
    ))
}

// POST /v1/evidence/collection — write impact_evidence rows from a collection file scan.
pub(crate) async fn ingest_collection_evidence(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
    Json(items): Json<Vec<CollectionEvidenceItem>>,
) -> Result<impl IntoResponse, ApiError> {
    if items.len() > 1000 {
        return Err(ApiError::TooManyRequests("batch too large, max 1000".into()));
    }

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();
    let now = Utc::now().to_rfc3339();
    let mut inserted = 0usize;

    for item in &items {
        let id = collection_evidence_id(
            &item.consumer_id,
            &item.service_id,
            &item.operation,
            &item.field_path,
        );

        let uri = if item.evidence_uri.is_empty() {
            format!("collection_file://{}#{}", item.service_id, item.consumer_id)
        } else {
            item.evidence_uri.clone()
        };

        // ON CONFLICT(id) DO NOTHING is atomic on both SQLite and PostgreSQL.
        // The deterministic UUID v5 id makes this idempotent across re-scans and restarts.
        let result = sqlx::query(
            "INSERT INTO impact_evidence \
             (id, org_id, diff_id, producer_service_id, consumer_id, source_type, \
              operation, field_path, confidence, evidence_uri, observed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&id)
        .bind(&org_id)
        .bind("")
        .bind(&item.service_id)
        .bind(&item.consumer_id)
        .bind("collection_file")
        .bind(if item.operation.is_empty() { None } else { Some(&item.operation) })
        .bind(if item.field_path.is_empty() { None } else { Some(&item.field_path) })
        .bind("medium")
        .bind(&uri)
        .bind(&now)
        .execute(&pool)
        .await?;

        if result.rows_affected() > 0 {
            inserted += 1;
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"accepted": items.len(), "inserted": inserted})),
    ))
}

// POST /v1/services/:id/subscriptions
pub(crate) async fn create_subscription(
    Path(service_id): Path<String>,
    State(pool): State<sqlx::AnyPool>,
    Json(body): Json<CreateSubscriptionBody>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    // Verify consumer exists.
    let consumer_exists = sqlx::query("SELECT id FROM consumer WHERE id = ?")
        .bind(&body.consumer_id)
        .fetch_optional(&pool)
        .await?;
    if consumer_exists.is_none() {
        return Err(ApiError::NotFound(format!(
            "consumer {} not found",
            body.consumer_id
        )));
    }

    // Verify service exists.
    let service_exists = sqlx::query("SELECT id FROM service WHERE id = ?")
        .bind(&service_id)
        .fetch_optional(&pool)
        .await?;
    if service_exists.is_none() {
        return Err(ApiError::NotFound(format!(
            "service {service_id} not found"
        )));
    }

    // Check for existing subscription — idempotent.
    let existing = sqlx::query(
        "SELECT id, service_id, consumer_id, opted_in_at FROM subscription WHERE service_id = ? AND consumer_id = ?",
    )
    .bind(&service_id)
    .bind(&body.consumer_id)
    .fetch_optional(&pool)
    .await?;

    if let Some(row) = existing {
        let resp = json!({
            "id":          row.get::<String, _>("id"),
            "service_id":  row.get::<String, _>("service_id"),
            "consumer_id": row.get::<String, _>("consumer_id"),
            "opted_in_at": row.get::<String, _>("opted_in_at"),
        });
        return Ok((StatusCode::OK, Json(resp)));
    }

    let sub_id = Uuid::new_v4().to_string();
    let opted_in_at = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO subscription (id, service_id, consumer_id, opted_in_at)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(&sub_id)
    .bind(&service_id)
    .bind(&body.consumer_id)
    .bind(&opted_in_at)
    .execute(&pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id":          sub_id,
            "service_id":  service_id,
            "consumer_id": body.consumer_id,
            "opted_in_at": opted_in_at,
        })),
    ))
}

// GET /v1/consumers — list all registered Consumer services
pub(crate) async fn list_all_consumers(
    State(pool): State<sqlx::AnyPool>,
    org: Option<axum::extract::Extension<JwtClaims>>,
) -> Result<impl IntoResponse, ApiError> {
    use sqlx::Row;

    let org_id = org.map(|e| e.org_id.clone()).unwrap_or_default();

    let rows = if !org_id.is_empty() {
        sqlx::query(
            r#"
            SELECT
                c.id, c.name, c.repo_url, c.owner_team, c.contact,
                (SELECT COUNT(*) FROM subscription s WHERE s.consumer_id = c.id)         AS subscription_count,
                (SELECT MAX(recorded_at) FROM usage_event ue WHERE ue.consumer_id = c.id) AS last_seen
            FROM consumer c
            WHERE c.org_id = ?
            ORDER BY c.name
            "#,
        )
        .bind(&org_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT
                c.id, c.name, c.repo_url, c.owner_team, c.contact,
                (SELECT COUNT(*) FROM subscription s WHERE s.consumer_id = c.id)         AS subscription_count,
                (SELECT MAX(recorded_at) FROM usage_event ue WHERE ue.consumer_id = c.id) AS last_seen
            FROM consumer c
            ORDER BY c.name
            "#,
        )
        .fetch_all(&pool)
        .await?
    };

    let items: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":                 row.get::<String, _>("id"),
                "name":               row.get::<String, _>("name"),
                "repo_url":           row.get::<String, _>("repo_url"),
                "owner_team":         row.get::<String, _>("owner_team"),
                "contact":            row.get::<String, _>("contact"),
                "subscription_count": row.try_get::<i64, _>("subscription_count").unwrap_or(0),
                "last_seen":          row.try_get::<Option<String>, _>("last_seen").unwrap_or(None),
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!(items))))
}
