mod embedding;
mod errors;
mod handler;
mod indexer;
mod model;
mod repository;
mod search;
mod service;
mod store;

pub use embedding::EmbeddingClient;
pub use errors::{KnowledgeError, Result};
pub use handler::routes;
pub use indexer::Indexer;
pub use model::*;
pub use repository::{MemoryStore, PgKnowledgeStore};
pub use search::{MemorySearchEngine, OpenSearchEngine, PgSearchEngine, SearchEngine};
pub use service::KnowledgeService;
pub use store::Store;
