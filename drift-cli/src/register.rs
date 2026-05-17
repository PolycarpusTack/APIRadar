use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct CreateConsumerRequest<'a> {
    name: &'a str,
    repo_url: &'a str,
    owner_team: &'a str,
    contact: &'a str,
}

#[derive(Deserialize)]
struct ConsumerResponse {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct SubscribeRequest<'a> {
    consumer_id: &'a str,
}

/// Register this repo as a consumer of a producer service.
/// Reads consumer info from .drift.yml (name, repo_url) and CLI args.
pub async fn run(
    api_url: &str,
    service_id: &str,
    consumer_name: &str,
    repo_url: &str,
    owner_team: &str,
    contact: &str,
    token: Option<&str>,
) -> Result<()> {
    let client = Client::new();

    // 1. Create (or find) the consumer
    let mut req = client
        .post(format!("{api_url}/v1/consumers"))
        .json(&CreateConsumerRequest {
            name: consumer_name,
            repo_url,
            owner_team,
            contact,
        });
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?.error_for_status()?;
    let consumer: ConsumerResponse = resp.json().await?;
    println!("Consumer registered: {} ({})", consumer.name, consumer.id);

    // 2. Subscribe consumer to the producer service
    let mut sub_req = client
        .post(format!("{api_url}/v1/services/{service_id}/subscriptions"))
        .json(&SubscribeRequest {
            consumer_id: &consumer.id,
        });
    if let Some(t) = token {
        sub_req = sub_req.bearer_auth(t);
    }
    let sub_resp = sub_req.send().await?;
    match sub_resp.status().as_u16() {
        200 => println!("Already subscribed to service {service_id}"),
        201 => println!("Subscribed to service {service_id}"),
        s => anyhow::bail!("Unexpected status {s} when subscribing"),
    }
    Ok(())
}
