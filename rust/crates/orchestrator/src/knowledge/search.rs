use async_trait::async_trait;
use reqwest::Client;
use sqlx::Row;

use super::errors::{KnowledgeError, Result};
use super::model::{KnowledgeEntry, SearchMode, SearchRequest, SearchResult};

#[async_trait]
pub trait SearchEngine: Send + Sync {
    async fn index(&self, entry: &KnowledgeEntry, embedding: Option<Vec<f32>>) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn search(&self, req: &SearchRequest, embedding: Option<Vec<f32>>) -> Result<Vec<SearchResult>>;
}

#[derive(Clone, Default)]
pub struct MemorySearchEngine {
    entries: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, KnowledgeEntry>>>,
}

impl MemorySearchEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SearchEngine for MemorySearchEngine {
    async fn index(&self, entry: &KnowledgeEntry, _embedding: Option<Vec<f32>>) -> Result<()> {
        self.entries.lock().await.insert(entry.id.clone(), entry.clone());
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        self.entries.lock().await.remove(id);
        Ok(())
    }

    async fn search(&self, req: &SearchRequest, _embedding: Option<Vec<f32>>) -> Result<Vec<SearchResult>> {
        let entries = self.entries.lock().await;
        let mut results = Vec::new();
        let query = req.query.to_lowercase();

        for entry in entries.values() {
            if let Some(org_id) = &req.org_id
                && &entry.org_id != org_id
            {
                continue;
            }
            if let Some(entry_type) = req.entry_type
                && entry.entry_type != entry_type
            {
                continue;
            }
            if !req.tags.is_empty() && !req.tags.iter().all(|tag| entry.tags.contains(tag)) {
                continue;
            }

            let haystack = format!("{} {}", entry.title, entry.content).to_lowercase();
            if haystack.contains(&query) {
                results.push(SearchResult { entry: entry.clone(), score: 1.0 });
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }
}

pub struct PgSearchEngine {
    pool: sqlx::PgPool,
}

impl PgSearchEngine {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchEngine for PgSearchEngine {
    async fn index(&self, _entry: &KnowledgeEntry, _embedding: Option<Vec<f32>>) -> Result<()> {
        Ok(())
    }

    async fn delete(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn search(&self, req: &SearchRequest, _embedding: Option<Vec<f32>>) -> Result<Vec<SearchResult>> {
        let org_id = req.org_id.clone().unwrap_or_default();
        let rows = sqlx::query(
            "SELECT id, type, title, content, source_id, source_type, source_ref, tags, org_id, created_by, embedding_status, created_at, updated_at, \
             ts_rank(to_tsvector('english', title || ' ' || content), plainto_tsquery('english', $1)) AS score \
             FROM knowledge_entries WHERE org_id = $2 AND to_tsvector('english', title || ' ' || content) @@ plainto_tsquery('english', $1) \
             ORDER BY score DESC LIMIT $3 OFFSET $4",
        )
        .bind(&req.query)
        .bind(&org_id)
        .bind(req.limit.max(20) as i64)
        .bind(req.offset as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let entry = KnowledgeEntry {
                id: row.try_get("id")?,
                entry_type: row.try_get::<String, _>("type")?.parse().unwrap_or(super::model::EntryType::Document),
                title: row.try_get("title")?,
                content: row.try_get("content")?,
                source_id: row.try_get("source_id")?,
                source_type: row.try_get("source_type")?,
                source_ref: row.try_get("source_ref")?,
                tags: row.try_get("tags")?,
                org_id: row.try_get("org_id")?,
                created_by: row.try_get("created_by")?,
                embedding_status: row
                    .try_get::<String, _>("embedding_status")?
                    .parse()
                    .unwrap_or(super::model::EmbeddingStatus::Pending),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            let score = row.try_get::<f64, _>("score")?;
            results.push(SearchResult { entry, score });
        }
        Ok(results)
    }
}

pub struct OpenSearchEngine {
    base_url: String,
    client: Client,
}

impl OpenSearchEngine {
    pub fn new(base_url: String) -> Self {
        Self { base_url, client: Client::new() }
    }
}

#[async_trait]
impl SearchEngine for OpenSearchEngine {
    async fn index(&self, entry: &KnowledgeEntry, embedding: Option<Vec<f32>>) -> Result<()> {
        let mut doc = serde_json::json!({
            "id": entry.id,
            "type": entry.entry_type,
            "title": entry.title,
            "content": entry.content,
            "source_id": entry.source_id,
            "source_type": entry.source_type,
            "source_ref": entry.source_ref,
            "tags": entry.tags,
            "org_id": entry.org_id,
            "created_by": entry.created_by,
            "embedding_status": entry.embedding_status,
            "created_at": entry.created_at,
            "updated_at": entry.updated_at,
        });
        if let Some(embedding) = embedding {
            doc["embedding"] = serde_json::to_value(embedding)?;
        }
        let url = format!("{}/knowledge_entries/_doc/{}", self.base_url.trim_end_matches('/'), entry.id);
        let resp = self.client.put(url).json(&doc).send().await?;
        if !resp.status().is_success() {
            return Err(KnowledgeError::Other(format!("open search index returned {}", resp.status())));
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let url = format!("{}/knowledge_entries/_doc/{}", self.base_url.trim_end_matches('/'), id);
        let _ = self.client.delete(url).send().await?;
        Ok(())
    }

    async fn search(&self, req: &SearchRequest, embedding: Option<Vec<f32>>) -> Result<Vec<SearchResult>> {
        let mode = req.mode;
        let body = match mode {
            SearchMode::Keyword => serde_json::json!({
                "query": {
                    "bool": {
                        "must": [
                            {"multi_match": {"query": req.query, "fields": ["title^2", "content"]}},
                            {"term": {"org_id": req.org_id.clone().unwrap_or_default()}}
                        ]
                    }
                },
                "size": req.limit.max(20),
                "from": req.offset
            }),
            SearchMode::Semantic => serde_json::json!({
                "query": {
                    "knn": {
                        "embedding": {
                            "vector": embedding.unwrap_or_default(),
                            "k": req.limit.max(20)
                        }
                    }
                },
                "size": req.limit.max(20),
                "from": req.offset
            }),
            SearchMode::Hybrid => serde_json::json!({
                "query": {
                    "bool": {
                        "must": [
                            {"multi_match": {"query": req.query, "fields": ["title^2", "content"]}},
                            {"term": {"org_id": req.org_id.clone().unwrap_or_default()}}
                        ]
                    }
                },
                "size": req.limit.max(20),
                "from": req.offset
            }),
        };
        let url = format!("{}/knowledge_entries/_search", self.base_url.trim_end_matches('/'));
        let resp = self.client.post(url).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(KnowledgeError::Other(format!("open search search returned {}", resp.status())));
        }
        Ok(Vec::new())
    }
}
