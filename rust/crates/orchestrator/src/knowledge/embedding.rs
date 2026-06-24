use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::errors::Result;

#[derive(Clone)]
pub struct EmbeddingClient {
    api_url: String,
    api_key: String,
    model: String,
    client: Client,
}

impl EmbeddingClient {
    pub fn new(api_url: String, api_key: String, model: String) -> Self {
        Self { api_url, api_key, model, client: Client::new() }
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = if self.api_url.ends_with("/v1") || self.api_url.ends_with("/v1/") {
            format!("{}/embeddings", self.api_url.trim_end_matches('/'))
        } else {
            format!("{}/v1/embeddings", self.api_url.trim_end_matches('/'))
        };
        let body = EmbeddingRequest { model: self.model.clone(), input: text.to_string() };
        let mut req = self.client.post(url).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            return Err(super::errors::KnowledgeError::Other(format!("embedding API returned {}", resp.status())));
        }
        let parsed: EmbeddingResponse = resp.json().await?;
        parsed
            .data
            .into_iter()
            .next()
            .map(|item| item.embedding)
            .ok_or_else(|| super::errors::KnowledgeError::Other("embedding response contained no data".to_string()))
    }
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}
