use async_trait::async_trait;

use super::errors::Result;
use super::model::{EmbeddingStatus, KnowledgeEntry, KnowledgeFilter, UpdateKnowledgeRequest};

#[async_trait]
pub trait Store: Send + Sync {
    async fn create(&self, entry: &mut KnowledgeEntry) -> Result<()>;
    async fn get_by_id(&self, id: &str, org_id: &str) -> Result<KnowledgeEntry>;
    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<KnowledgeEntry>>;
    async fn update(&self, id: &str, org_id: &str, req: UpdateKnowledgeRequest) -> Result<()>;
    async fn delete(&self, id: &str, org_id: &str) -> Result<()>;
    async fn update_embedding_status(&self, id: &str, status: EmbeddingStatus) -> Result<()>;
}
