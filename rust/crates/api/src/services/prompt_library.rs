//! Prompt library service — validation and management of stored prompt templates.

use agentforge_core::{AppResult, TenantScope};
use agentforge_db::entities::Prompt;
use uuid::Uuid;

use crate::domain::prompt_library::PromptTemplatePolicy;
use crate::repositories::prompt::PromptRepository;

/// Business logic layer for prompt library operations (stored prompt templates).
pub struct PromptLibraryService {
    repo: PromptRepository,
}

impl PromptLibraryService {
    pub fn new(repo: PromptRepository) -> Self {
        Self { repo }
    }

    /// List prompts visible to the user.
    pub async fn list(
        &self,
        scope: &TenantScope,
        shared_only: Option<bool>,
        tags: Option<Vec<String>>,
    ) -> AppResult<Vec<Prompt>> {
        self.repo.list(scope, shared_only, tags.as_deref()).await
    }

    /// Get a single prompt by ID.
    pub async fn get(&self, scope: &TenantScope, id: Uuid) -> AppResult<Prompt> {
        self.repo.get(scope, id).await
    }

    /// Create a new prompt after validation.
    pub async fn create(
        &self,
        scope: &TenantScope,
        title: &str,
        content: &str,
        tags: &[String],
        is_shared: bool,
    ) -> AppResult<Prompt> {
        PromptTemplatePolicy::validate_create(title, content, tags)?;
        self.repo.create(scope, title, content, tags, is_shared).await
    }

    /// Update an existing prompt.
    pub async fn update(
        &self,
        scope: &TenantScope,
        id: Uuid,
        title: Option<&str>,
        content: Option<&str>,
        tags: Option<&[String]>,
        is_shared: Option<bool>,
    ) -> AppResult<Prompt> {
        PromptTemplatePolicy::validate_update(title, content, tags)?;
        self.repo.update(scope, id, title, content, tags, is_shared).await
    }

    /// Delete a prompt.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}
