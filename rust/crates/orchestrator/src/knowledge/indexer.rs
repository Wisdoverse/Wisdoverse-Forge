use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use tokio::task;
use tracing::warn;

use super::embedding::EmbeddingClient;
use super::errors::Result;
use super::model::{EmbeddingStatus, KnowledgeEntry};
use super::search::SearchEngine;
use super::store::Store;

const BUFFER_SIZE: usize = 256;
const WORKERS: usize = 4;

#[derive(Clone)]
pub struct Indexer {
    sender: mpsc::Sender<KnowledgeEntry>,
    store: Arc<dyn Store>,
    search: Arc<dyn SearchEngine>,
    embedder: Option<Arc<EmbeddingClient>>,
}

impl Indexer {
    pub fn new(
        store: Arc<dyn Store>,
        search: Arc<dyn SearchEngine>,
        embedder: Option<Arc<EmbeddingClient>>,
    ) -> Arc<Self> {
        let (sender, receiver) = mpsc::channel(BUFFER_SIZE);
        let indexer = Arc::new(Self { sender, store, search, embedder });
        let shared_rx = Arc::new(Mutex::new(receiver));

        for _ in 0..WORKERS {
            let indexer = Arc::clone(&indexer);
            let rx = Arc::clone(&shared_rx);
            task::spawn(async move {
                indexer.worker(rx).await;
            });
        }

        indexer
    }

    pub fn submit(&self, entry: KnowledgeEntry) -> bool {
        self.sender.try_send(entry).is_ok()
    }

    async fn worker(self: Arc<Self>, receiver: Arc<Mutex<mpsc::Receiver<KnowledgeEntry>>>) {
        loop {
            let job = {
                let mut rx = receiver.lock().await;
                rx.recv().await
            };

            let Some(entry) = job else {
                return;
            };

            task::yield_now().await;
            if let Err(err) = self.process(entry).await {
                warn!(error = %err, "knowledge indexer job failed");
            }
        }
    }

    async fn process(&self, entry: KnowledgeEntry) -> Result<()> {
        self.store.update_embedding_status(&entry.id, EmbeddingStatus::Processing).await?;

        let embedding = match &self.embedder {
            Some(embedder) => match embedder.embed(&(entry.title.clone() + "\n\n" + &entry.content)).await {
                Ok(vec) => Some(vec),
                Err(err) => {
                    self.store.update_embedding_status(&entry.id, EmbeddingStatus::Failed).await?;
                    return Err(err);
                }
            },
            None => None,
        };

        self.search.index(&entry, embedding).await?;
        self.store.update_embedding_status(&entry.id, EmbeddingStatus::Completed).await?;
        Ok(())
    }
}
