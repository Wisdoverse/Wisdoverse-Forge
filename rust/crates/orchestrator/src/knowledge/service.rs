use std::sync::Arc;

use super::embedding::EmbeddingClient;
use super::errors::Result;
use super::indexer::Indexer;
use super::model::{
    EmbeddingStatus, KnowledgeEntry, KnowledgeFilter, SearchMode, SearchRequest, SearchResponse, UpdateKnowledgeRequest,
};
use super::repository::{MemoryStore, PgKnowledgeStore};
use super::search::{MemorySearchEngine, OpenSearchEngine, PgSearchEngine, SearchEngine};
use super::store::Store;
use crate::config::Config;

pub struct KnowledgeService {
    store: Arc<dyn Store>,
    search: Arc<dyn SearchEngine>,
    indexer: Arc<Indexer>,
    embedder: Option<Arc<EmbeddingClient>>,
}

impl KnowledgeService {
    pub fn test() -> Self {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let search: Arc<dyn SearchEngine> = Arc::new(MemorySearchEngine::new());
        let indexer = Indexer::new(Arc::clone(&store), Arc::clone(&search), None);
        Self { store, search, indexer, embedder: None }
    }

    pub async fn live(pool: sqlx::PgPool, config: &Config) -> Result<Self> {
        let store: Arc<dyn Store> = Arc::new(PgKnowledgeStore::new(pool.clone()));
        let search: Arc<dyn SearchEngine> = if config.opensearch_enabled {
            Arc::new(OpenSearchEngine::new(config.opensearch_url.clone()))
        } else {
            Arc::new(PgSearchEngine::new(pool))
        };
        let embedder = if config.embedding_api_url.is_empty() {
            None
        } else {
            Some(Arc::new(EmbeddingClient::new(
                config.embedding_api_url.clone(),
                config.embedding_api_key.clone(),
                config.embedding_model.clone(),
            )))
        };
        let indexer = Indexer::new(Arc::clone(&store), Arc::clone(&search), embedder.clone());
        Ok(Self { store, search, indexer, embedder })
    }

    pub async fn create(&self, entry: &mut KnowledgeEntry) -> Result<()> {
        self.store.create(entry).await?;
        let _ = self.indexer.submit(entry.clone());
        Ok(())
    }

    pub async fn get_by_id(&self, id: &str, org_id: &str) -> Result<KnowledgeEntry> {
        self.store.get_by_id(id, org_id).await
    }

    pub async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<KnowledgeEntry>> {
        self.store.list(filter).await
    }

    pub async fn update(&self, id: &str, org_id: &str, req: UpdateKnowledgeRequest) -> Result<KnowledgeEntry> {
        self.store.update(id, org_id, req).await?;
        let mut entry = self.store.get_by_id(id, org_id).await?;
        self.store.update_embedding_status(id, EmbeddingStatus::Pending).await?;
        entry.embedding_status = EmbeddingStatus::Pending;
        let _ = self.indexer.submit(entry.clone());
        Ok(entry)
    }

    pub async fn delete(&self, id: &str, org_id: &str) -> Result<()> {
        self.store.delete(id, org_id).await?;
        let _ = self.search.delete(id).await;
        Ok(())
    }

    pub async fn search(&self, mut req: SearchRequest) -> Result<SearchResponse> {
        let requested_mode = req.mode;
        let mut actual_mode = requested_mode;
        let mut degraded = false;
        let mut degraded_reason = None;
        let embedding = match requested_mode {
            SearchMode::Semantic | SearchMode::Hybrid => match &self.embedder {
                Some(embedder) => Some(embedder.embed(&req.query).await?),
                None => {
                    actual_mode = SearchMode::Keyword;
                    degraded = true;
                    degraded_reason = Some("embedding service unavailable, results are keyword-only".to_string());
                    None
                }
            },
            SearchMode::Keyword => None,
        };

        req.mode = actual_mode;
        let results = self.search.search(&req, embedding).await?;
        Ok(SearchResponse { results, requested_mode, actual_mode, degraded, degraded_reason })
    }

    pub async fn bulk_index(&self, org_id: &str) -> Result<usize> {
        let entries = self
            .store
            .list(KnowledgeFilter {
                org_id: org_id.to_string(),
                entry_type: None,
                tags: vec![],
                status: Some(EmbeddingStatus::Pending),
                limit: 1000,
                offset: 0,
            })
            .await?;

        let mut submitted = 0;
        for entry in entries {
            if self.indexer.submit(entry) {
                submitted += 1;
            }
        }
        Ok(submitted)
    }
}
