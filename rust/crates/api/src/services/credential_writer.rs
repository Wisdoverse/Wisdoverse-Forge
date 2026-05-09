//! Shim so `agentforge-jobs::credential_consumer::CredentialWriter` can be
//! satisfied by `CliCredentialService` without creating a circular dep:
//! `agentforge-api` already depends on `agentforge-jobs`; the reverse edge
//! would not link. Task 7 (server bootstrap) constructs this.

use std::sync::Arc;

use agentforge_jobs::CredentialWriter;
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::services::cli_credential::CliCredentialService;

#[derive(Clone)]
pub struct ServiceCredentialWriter {
    service: Arc<CliCredentialService>,
}

impl ServiceCredentialWriter {
    pub fn new(service: Arc<CliCredentialService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl CredentialWriter for ServiceCredentialWriter {
    async fn upsert(&self, user_id: Uuid, cli_tool: &str, plaintext_json: &str) -> Result<()> {
        self.service
            .upsert_encrypted_by_user_id(user_id, cli_tool, plaintext_json)
            .await
            // Preserve the source chain via ErrorKind (implements std::error::Error via thiserror).
            .map_err(|e| anyhow::Error::new(e.kind))
    }
}
