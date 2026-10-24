//! Authenticated model metadata. Never persist the raw catalog: it can include
//! instructions and unrelated account metadata. No response bodies enter logs.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;

use super::auth::{
    constants::{CODEX_API_ENDPOINT, RESPONSES_LITE_ORIGINATOR},
    manager::CodexAuthManager,
    token_store::{StoredAuth, file_store},
};
use super::translate::model_allowlist::ALLOWED_MODELS;

const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
pub const CLIENT_VERSION: &str = "0.164.0";
pub const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub id: String,
    pub context_window: u64,
    pub max_context_window: Option<u64>,
    pub default_effort: Option<String>,
    pub supported_efforts: Vec<String>,
    pub auto_compact_tokens: u64,
}

impl ModelMetadata {
    pub fn anthropic_info(&self) -> Value {
        let mut effort = serde_json::Map::from_iter([(
            "supported".into(),
            json!(!self.supported_efforts.is_empty()),
        )]);
        for level in EFFORTS {
            effort.insert(
                (*level).into(),
                json!({"supported":self.supported_efforts.iter().any(|e| e == level)}),
            );
        }
        json!({"type":"model", "object":"model", "id":self.id,
            "display_name":format!("{} (Codex, {}k context)", self.id, self.context_window / 1000),
            "max_input_tokens":self.context_window,
            "capabilities":{"effort":effort}, "proxy_metadata":self})
    }
}

pub fn catalog_url(base: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(base)?;
    let local = url
        .host_str()
        .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback());
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && local),
        "Catalog requires HTTPS or a literal loopback HTTP endpoint"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Catalog base URL must not contain credentials, query or fragment"
    );
    let path = url.path().trim_end_matches('/');
    let root = path.strip_suffix("/responses").unwrap_or(path);
    url.set_path(&format!("{root}/models"));
    url.query_pairs_mut()
        .append_pair("client_version", CLIENT_VERSION);
    Ok(url)
}

pub fn parse_catalog(bytes: &[u8]) -> Result<Vec<ModelMetadata>> {
    ensure!(
        bytes.len() <= MAX_CATALOG_BYTES,
        "Model catalog exceeds size limit"
    );
    let raw: Value =
        serde_json::from_slice(bytes).map_err(|_| anyhow::anyhow!("Invalid model catalog JSON"))?;
    let entries = raw
        .get("models")
        .or_else(|| raw.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("Model catalog has no model list"))?;
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let Some(id) = entry
            .get("slug")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        // Discovery does not expand the proxy's translation allowlist.
        if !ALLOWED_MODELS.contains(&id) || !seen.insert(id.to_owned()) {
            continue;
        }
        let Some(window) = entry
            .get("context_window")
            .and_then(Value::as_u64)
            .filter(|n| (1..=10_000_000).contains(n))
        else {
            continue;
        };
        let max = entry
            .get("max_context_window")
            .and_then(Value::as_u64)
            .filter(|n| *n >= window && *n <= 10_000_000);
        let efforts: Vec<String> = entry
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|e| e.get("effort").and_then(Value::as_str))
            .filter(|e| EFFORTS.contains(e))
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let default = entry
            .get("default_reasoning_level")
            .and_then(Value::as_str)
            .filter(|e| efforts.iter().any(|supported| supported == e))
            .map(str::to_owned);
        // A proxy safety policy, not a claim about the backend's effective limit.
        let percent = entry
            .get("effective_context_window_percent")
            .and_then(Value::as_u64)
            .filter(|p| (1..=100).contains(p))
            .unwrap_or(95)
            .min(95);
        result.push(ModelMetadata {
            id: id.into(),
            context_window: window,
            max_context_window: max,
            default_effort: default,
            supported_efforts: efforts,
            auto_compact_tokens: window * percent / 100,
        });
    }
    ensure!(
        !result.is_empty(),
        "Catalog contains no compatible models with valid context metadata"
    );
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

pub async fn fetch_with_auth(
    client: &reqwest::Client,
    endpoint: url::Url,
    auth: &StoredAuth,
) -> Result<Vec<ModelMetadata>> {
    let mut request = client
        .get(endpoint)
        .bearer_auth(&auth.access)
        .header("originator", RESPONSES_LITE_ORIGINATOR);
    if let Some(account) = &auth.account_id {
        request = request.header("ChatGPT-Account-Id", account);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Model catalog transport failed"))?;
    ensure!(
        response.status().is_success(),
        "Model catalog returned HTTP {}",
        response.status().as_u16()
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|n| n <= MAX_CATALOG_BYTES as u64),
        "Model catalog exceeds size limit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("Model catalog read failed"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
            bail!("Model catalog exceeds size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_catalog(&bytes)
}

pub async fn fetch_catalog() -> Result<Vec<ModelMetadata>> {
    let endpoint = catalog_url(&crate::config::codex_base_url(CODEX_API_ENDPOINT))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()?;
    let auth = CodexAuthManager::new(file_store()).get_auth().await?;
    fetch_with_auth(&client, endpoint, &auth).await
}

#[derive(Default)]
pub struct CatalogCache(tokio::sync::Mutex<CachedCatalog>);
type CachedCatalog = Option<(std::time::Instant, Option<Vec<ModelMetadata>>)>;

impl CatalogCache {
    pub async fn get(&self) -> Result<Vec<ModelMetadata>> {
        let mut cache = self.0.lock().await;
        if let Some((at, models)) = cache.as_ref() {
            let ttl = if models.is_some() { 300 } else { 30 };
            if at.elapsed() < Duration::from_secs(ttl) {
                return models.clone().ok_or_else(|| {
                    anyhow::anyhow!("Model catalog unavailable; retry after the short backoff")
                });
            }
        }
        match fetch_catalog().await {
            Ok(models) => {
                *cache = Some((std::time::Instant::now(), Some(models.clone())));
                Ok(models)
            }
            Err(error) => {
                *cache = Some((std::time::Instant::now(), None));
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_bounded_filtered_and_does_not_use_the_maximum_as_default() {
        let models = parse_catalog(br#"{"models":[{"slug":"gpt-6-sol","context_window":272000,"max_context_window":872000,"instructions":"sensitive","default_reasoning_level":"medium","supported_reasoning_levels":[{"effort":"medium"},{"effort":"ultra"}]},{"slug":"untrusted-model","context_window":999999},{"slug":"gpt-6.1-sol","context_window":0}]}"#).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].context_window, 272000);
        assert_eq!(models[0].auto_compact_tokens, 258400);
        assert_eq!(models[0].supported_efforts, ["medium"]);
        assert!(
            !serde_json::to_string(&models)
                .unwrap()
                .contains("sensitive")
        );
        assert!(parse_catalog(&vec![b' '; MAX_CATALOG_BYTES + 1]).is_err());
        assert!(parse_catalog(br#"{"models":[]}"#).is_err());
    }
    #[test]
    fn endpoint_rejects_unsafe_destinations_and_preserves_origin() {
        for base in [
            "http://example.com/responses",
            "https://user:password@example.com/responses",
            "https://example.com/responses?secret=yes",
            "file:///tmp/catalog",
        ] {
            assert!(catalog_url(base).is_err());
        }
        assert_eq!(
            catalog_url("https://example.com/backend/responses")
                .unwrap()
                .as_str(),
            "https://example.com/backend/models?client_version=0.164.0"
        );
        assert!(catalog_url("http://127.0.0.1:9876/responses").is_ok());
    }
    #[tokio::test]
    async fn redirects_do_not_receive_credentials_and_error_bodies_are_not_exposed() {
        use axum::{Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/models",
            get(|| async {
                (
                    http::StatusCode::FOUND,
                    [("location", "https://example.invalid/leak")],
                    "synthetic-sensitive-error",
                )
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let auth = StoredAuth {
            access: "synthetic".into(),
            refresh: "synthetic".into(),
            expires: u64::MAX,
            account_id: None,
        };
        let error = fetch_with_auth(
            &client,
            format!("http://{address}/models").parse().unwrap(),
            &auth,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("302"));
        assert!(!error.contains("synthetic"));
        task.abort();
    }
}
