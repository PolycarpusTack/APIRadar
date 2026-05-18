use anyhow::Result;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Postman Collection v2.1 types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct Collection {
    pub info: Info,
    pub item: Vec<Item>,
    pub variable: Vec<Variable>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Info {
    pub name: String,
    pub _postman_id: String,
    pub schema: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
    pub name: String,
    pub event: Vec<Event>,
    pub request: PostmanRequest,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Event {
    pub listen: String,
    pub script: Script,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Script {
    #[serde(rename = "type")]
    pub script_type: String,
    pub exec: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PostmanRequest {
    pub method: String,
    pub header: Vec<Header>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    pub url: Url,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Header {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub header_type: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Body {
    pub mode: String,
    pub raw: String,
    pub options: BodyOptions,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BodyOptions {
    pub raw: RawOptions,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RawOptions {
    pub language: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Url {
    pub raw: String,
    pub host: Vec<String>,
    pub path: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub query: Vec<QueryParam>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QueryParam {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Variable {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub var_type: String,
}

// ---------------------------------------------------------------------------
// Postman API push
// ---------------------------------------------------------------------------

/// Push a collection to a Postman workspace and return the collection URL.
/// Requires POSTMAN_API_KEY in the environment.
pub async fn push_collection(
    api_key: &str,
    workspace_id: Option<&str>,
    collection: &Collection,
) -> Result<String> {
    let mut req = reqwest::Client::new()
        .post("https://api.getpostman.com/collections")
        .header("X-Api-Key", api_key)
        .header("Content-Type", "application/json");

    if let Some(wid) = workspace_id {
        req = req.query(&[("workspace", wid)]);
    }

    let body = serde_json::json!({"collection": collection});
    let resp = req.json(&body).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Postman API error {status}: {err}"));
    }

    let data: serde_json::Value = resp.json().await?;
    let uid = data["collection"]["uid"].as_str().unwrap_or("");
    Ok(format!("https://www.postman.com/collection/{uid}"))
}
