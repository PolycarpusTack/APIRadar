use axum::{extract::State, response::IntoResponse, Json};
use serde_json::{json, Value};
use sqlx::AnyPool;

async fn count_table(pool: &AnyPool, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM \"{table}\""))
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

pub(crate) async fn get_readiness(State(pool): State<AnyPool>) -> impl IntoResponse {
    let services = count_table(&pool, "service").await;
    let diffs = count_table(&pool, "diff").await;
    let consumers = count_table(&pool, "consumer").await;
    let catalog_sources = count_table(&pool, "catalog_source").await;
    let webhooks = count_table(&pool, "webhook").await;

    let last_diff_at: Option<String> = qs!("SELECT MAX(created_at) FROM \"diff\"")
        .fetch_optional(&pool)
        .await
        .unwrap_or(None)
        .flatten();

    fn item(name: &str, status: &str, hint: &str, count: i64, extra: Option<Value>) -> Value {
        let mut v = json!({ "name": name, "status": status, "hint": hint, "count": count });
        if let Some(e) = extra {
            if let (Some(obj), Some(ext)) = (v.as_object_mut(), e.as_object()) {
                for (k, val) in ext {
                    obj.insert(k.clone(), val.clone());
                }
            }
        }
        v
    }

    let diff_status = if diffs > 0 {
        "ok"
    } else if services > 0 {
        "missing"
    } else {
        "warn"
    };

    let items: Vec<Value> = vec![
        item("db_connected", "ok", "", 1, None),
        item(
            "service_registered",
            if services > 0 { "ok" } else { "missing" },
            if services == 0 {
                "Register a service: radar check --service-id <uuid> --base old.yaml --head new.yaml"
            } else {
                ""
            },
            services,
            None,
        ),
        item(
            "diff_recorded",
            diff_status,
            if diffs == 0 {
                "Run radar check on a pull request to record your first diff"
            } else {
                ""
            },
            diffs,
            Some(json!({ "last_at": last_diff_at })),
        ),
        item(
            "consumer_registered",
            if consumers > 0 { "ok" } else { "missing" },
            if consumers == 0 {
                "Register a consumer: radar register --consumer-name checkout-svc --service-id <uuid>"
            } else {
                ""
            },
            consumers,
            None,
        ),
        item(
            "catalog_source_configured",
            if catalog_sources > 0 { "ok" } else { "warn" },
            if catalog_sources == 0 {
                "Optional: add a catalog source to auto-import services from Backstage or a YAML file"
            } else {
                ""
            },
            catalog_sources,
            None,
        ),
        item(
            "webhook_configured",
            if webhooks > 0 { "ok" } else { "warn" },
            if webhooks == 0 {
                "Optional: add a webhook to get Slack or HTTP alerts on breaking changes"
            } else {
                ""
            },
            webhooks,
            None,
        ),
    ];

    let critical_ok = services > 0 && diffs > 0 && consumers > 0;

    Json(json!({
        "overall": if critical_ok { "ready" } else { "setup_required" },
        "items": items,
    }))
}
