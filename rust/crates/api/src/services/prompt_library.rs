//! Prompt library service — validation and management of stored prompt templates.

use agentforge_core::{AppResult, ErrorKind, TenantScope};
use agentforge_db::entities::Prompt;
use uuid::Uuid;

use crate::repositories::prompt::PromptRepository;

/// Maximum title length.
const MAX_TITLE_LEN: usize = 200;
/// Maximum content length.
const MAX_CONTENT_LEN: usize = 50_000;
/// Maximum number of tags.
const MAX_TAGS: usize = 20;

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
        if title.is_empty() || title.len() > MAX_TITLE_LEN {
            return Err(ErrorKind::Validation(format!("title must be 1-{MAX_TITLE_LEN} characters")).into());
        }
        if content.is_empty() || content.len() > MAX_CONTENT_LEN {
            return Err(ErrorKind::Validation(format!("content must be 1-{MAX_CONTENT_LEN} characters")).into());
        }
        if tags.len() > MAX_TAGS {
            return Err(ErrorKind::Validation(format!("at most {MAX_TAGS} tags allowed")).into());
        }
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
        if let Some(t) = title
            && (t.is_empty() || t.len() > MAX_TITLE_LEN)
        {
            return Err(ErrorKind::Validation(format!("title must be 1-{MAX_TITLE_LEN} characters")).into());
        }
        if let Some(c) = content
            && (c.is_empty() || c.len() > MAX_CONTENT_LEN)
        {
            return Err(ErrorKind::Validation(format!("content must be 1-{MAX_CONTENT_LEN} characters")).into());
        }
        if let Some(t) = tags
            && t.len() > MAX_TAGS
        {
            return Err(ErrorKind::Validation(format!("at most {MAX_TAGS} tags allowed")).into());
        }
        self.repo.update(scope, id, title, content, tags, is_shared).await
    }

    /// Delete a prompt.
    pub async fn delete(&self, scope: &TenantScope, id: Uuid) -> AppResult<()> {
        self.repo.delete(scope, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_length_limits() {
        let max_title_len = MAX_TITLE_LEN;
        assert_eq!(max_title_len, 200);
    }

    #[test]
    fn content_length_limits() {
        let max_content_len = MAX_CONTENT_LEN;
        assert_eq!(max_content_len, 50_000);
    }

    #[test]
    fn max_tags_limit() {
        let max_tags = MAX_TAGS;
        assert_eq!(max_tags, 20);
    }
}
