use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, QueryBuilder, Row};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::errors::{KnowledgeError, Result};
use super::model::{EmbeddingStatus, EntryType, KnowledgeEntry, KnowledgeFilter, UpdateKnowledgeRequest};
use super::store::Store;

#[derive(Clone)]
pub struct MemoryStore {
    entries: Arc<Mutex<HashMap<String, KnowledgeEntry>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self { entries: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub async fn insert_pending(&self, mut entry: KnowledgeEntry) -> Result<KnowledgeEntry> {
        self.create(&mut entry).await?;
        Ok(entry)
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn create(&self, entry: &mut KnowledgeEntry) -> Result<()> {
        let now = Utc::now();
        if entry.id.is_empty() {
            entry.id = Uuid::now_v7().to_string();
        }
        entry.embedding_status = EmbeddingStatus::Pending;
        entry.created_at = now;
        entry.updated_at = now;
        self.entries.lock().await.insert(entry.id.clone(), entry.clone());
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<KnowledgeEntry> {
        self.entries
            .lock()
            .await
            .get(id)
            .filter(|entry| entry.org_id == org_id)
            .cloned()
            .ok_or(KnowledgeError::NotFound)
    }

    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<KnowledgeEntry>> {
        let mut entries: Vec<_> =
            self.entries.lock().await.values().filter(|entry| entry.org_id == filter.org_id).cloned().collect();

        if let Some(entry_type) = filter.entry_type {
            entries.retain(|entry| entry.entry_type == entry_type);
        }
        if let Some(status) = filter.status {
            entries.retain(|entry| entry.embedding_status == status);
        }
        if !filter.tags.is_empty() {
            entries.retain(|entry| filter.tags.iter().all(|tag| entry.tags.contains(tag)));
        }

        entries.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
        let start = filter.offset.min(entries.len());
        let end = (start + filter.limit.max(1)).min(entries.len());
        Ok(entries[start..end].to_vec())
    }

    async fn update(&self, id: &str, org_id: &str, req: UpdateKnowledgeRequest) -> Result<()> {
        let mut entries = self.entries.lock().await;
        let entry = entries.get_mut(id).filter(|entry| entry.org_id == org_id).ok_or(KnowledgeError::NotFound)?;

        if let Some(title) = req.title {
            entry.title = title;
        }
        if let Some(content) = req.content {
            entry.content = content;
        }
        if let Some(source_type) = req.source_type {
            entry.source_type = Some(source_type);
        }
        if let Some(source_ref) = req.source_ref {
            entry.source_ref = Some(source_ref);
        }
        if let Some(tags) = req.tags {
            entry.tags = tags;
        }
        entry.updated_at = Utc::now();
        Ok(())
    }

    async fn delete(&self, id: &str, org_id: &str) -> Result<()> {
        let mut entries = self.entries.lock().await;
        let should_remove = entries.get(id).map(|entry| entry.org_id == org_id).unwrap_or(false);
        if should_remove {
            entries.remove(id);
            Ok(())
        } else {
            Err(KnowledgeError::NotFound)
        }
    }

    async fn update_embedding_status(&self, id: &str, status: EmbeddingStatus) -> Result<()> {
        let mut entries = self.entries.lock().await;
        let entry = entries.get_mut(id).ok_or(KnowledgeError::NotFound)?;
        entry.embedding_status = status;
        entry.updated_at = Utc::now();
        Ok(())
    }
}

pub struct PgKnowledgeStore {
    pool: PgPool,
}

impl PgKnowledgeStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for PgKnowledgeStore {
    async fn create(&self, entry: &mut KnowledgeEntry) -> Result<()> {
        let row = sqlx::query(
            "INSERT INTO knowledge_entries (type, title, content, source_id, source_type, source_ref, tags, org_id, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             RETURNING id, embedding_status, created_at, updated_at",
        )
        .bind(entry.entry_type.to_string())
        .bind(&entry.title)
        .bind(&entry.content)
        .bind(&entry.source_id)
        .bind(&entry.source_type)
        .bind(&entry.source_ref)
        .bind(&entry.tags)
        .bind(&entry.org_id)
        .bind(&entry.created_by)
        .fetch_one(&self.pool)
        .await?;

        entry.id = row.try_get("id")?;
        entry.embedding_status = parse_embedding_status(row.try_get::<String, _>("embedding_status")?);
        entry.created_at = row.try_get("created_at")?;
        entry.updated_at = row.try_get("updated_at")?;
        Ok(())
    }

    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<KnowledgeEntry> {
        let row = sqlx::query(
            "SELECT id, type, title, content, source_id, source_type, source_ref, tags, org_id, created_by, embedding_status, created_at, updated_at \
             FROM knowledge_entries WHERE id = $1 AND org_id = $2",
        )
        .bind(id)
        .bind(org_id)
        .fetch_optional(&self.pool)
        .await?;

        let row = row.ok_or(KnowledgeError::NotFound)?;
        row_to_entry(&row)
    }

    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<KnowledgeEntry>> {
        let rows = sqlx::query(
            "SELECT id, type, title, content, source_id, source_type, source_ref, tags, org_id, created_by, embedding_status, created_at, updated_at \
             FROM knowledge_entries WHERE org_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&filter.org_id)
        .bind(filter.limit.max(1) as i64)
        .bind(filter.offset as i64);

        let rows = rows.fetch_all(&self.pool).await?;
        let mut entries = Vec::new();
        for row in rows {
            let entry = row_to_entry(&row)?;
            if let Some(entry_type) = filter.entry_type
                && entry.entry_type != entry_type
            {
                continue;
            }
            if let Some(status) = filter.status
                && entry.embedding_status != status
            {
                continue;
            }
            if !filter.tags.is_empty() && !filter.tags.iter().all(|tag| entry.tags.contains(tag)) {
                continue;
            }
            entries.push(entry);
        }
        Ok(entries)
    }

    async fn update(&self, id: &str, org_id: &str, req: UpdateKnowledgeRequest) -> Result<()> {
        if req.title.is_none()
            && req.content.is_none()
            && req.source_type.is_none()
            && req.source_ref.is_none()
            && req.tags.is_none()
        {
            return Ok(());
        }

        let mut qb = QueryBuilder::<sqlx::Postgres>::new("UPDATE knowledge_entries SET updated_at = NOW()");
        if let Some(title) = req.title {
            qb.push(", title = ").push_bind(title);
        }
        if let Some(content) = req.content {
            qb.push(", content = ").push_bind(content);
        }
        if let Some(source_type) = req.source_type {
            qb.push(", source_type = ").push_bind(source_type);
        }
        if let Some(source_ref) = req.source_ref {
            qb.push(", source_ref = ").push_bind(source_ref);
        }
        if let Some(tags) = req.tags {
            qb.push(", tags = ").push_bind(tags);
        }
        qb.push(" WHERE id = ").push_bind(id).push(" AND org_id = ").push_bind(org_id);
        let result = qb.build().execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            return Err(KnowledgeError::NotFound);
        }
        Ok(())
    }

    async fn delete(&self, id: &str, org_id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM knowledge_entries WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(org_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(KnowledgeError::NotFound);
        }
        Ok(())
    }

    async fn update_embedding_status(&self, id: &str, status: EmbeddingStatus) -> Result<()> {
        let result =
            sqlx::query("UPDATE knowledge_entries SET embedding_status = $1, updated_at = NOW() WHERE id = $2")
                .bind(status.to_string())
                .bind(id)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(KnowledgeError::NotFound);
        }
        Ok(())
    }
}

fn row_to_entry(row: &sqlx::postgres::PgRow) -> Result<KnowledgeEntry> {
    Ok(KnowledgeEntry {
        id: row.try_get("id")?,
        entry_type: parse_entry_type(row.try_get::<String, _>("type")?),
        title: row.try_get("title")?,
        content: row.try_get("content")?,
        source_id: row.try_get("source_id")?,
        source_type: row.try_get("source_type")?,
        source_ref: row.try_get("source_ref")?,
        tags: row.try_get("tags")?,
        org_id: row.try_get("org_id")?,
        created_by: row.try_get("created_by")?,
        embedding_status: parse_embedding_status(row.try_get::<String, _>("embedding_status")?),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn parse_entry_type(value: String) -> EntryType {
    match value.as_str() {
        "session" => EntryType::Session,
        "document" => EntryType::Document,
        "snippet" => EntryType::Snippet,
        "session_summary" => EntryType::SessionSummary,
        "review_learnings" => EntryType::ReviewLearnings,
        "decision_record" => EntryType::DecisionRecord,
        _ => EntryType::Document,
    }
}

fn parse_embedding_status(value: String) -> EmbeddingStatus {
    match value.as_str() {
        "processing" => EmbeddingStatus::Processing,
        "completed" => EmbeddingStatus::Completed,
        "failed" => EmbeddingStatus::Failed,
        _ => EmbeddingStatus::Pending,
    }
}
