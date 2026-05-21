//! Prompt library domain rules.
//!
//! This module owns stored prompt template policies that are independent of
//! repositories, HTTP route DTOs, and persistence details.

use agentforge_core::{AppResult, ErrorKind};
use serde::Serialize;
use serde_json::{Value, json};

const MAX_TITLE_LEN: usize = 200;
const MAX_CONTENT_LEN: usize = 50_000;
const MAX_TAGS: usize = 20;

pub(crate) fn prompt_library_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn prompt_library_delete_response() -> Value {
    json!({ "ok": true })
}

/// Stored prompt template validation policy.
pub(crate) struct PromptTemplatePolicy;

impl PromptTemplatePolicy {
    pub(crate) fn validate_create(title: &str, content: &str, tags: &[String]) -> AppResult<()> {
        Self::validate_title(title)?;
        Self::validate_content(content)?;
        Self::validate_tags(tags)
    }

    pub(crate) fn validate_update(
        title: Option<&str>,
        content: Option<&str>,
        tags: Option<&[String]>,
    ) -> AppResult<()> {
        if let Some(title) = title {
            Self::validate_title(title)?;
        }
        if let Some(content) = content {
            Self::validate_content(content)?;
        }
        if let Some(tags) = tags {
            Self::validate_tags(tags)?;
        }
        Ok(())
    }

    fn validate_title(title: &str) -> AppResult<()> {
        if title.is_empty() || title.len() > MAX_TITLE_LEN {
            return Err(ErrorKind::Validation(format!("title must be 1-{MAX_TITLE_LEN} characters")).into());
        }
        Ok(())
    }

    fn validate_content(content: &str) -> AppResult<()> {
        if content.is_empty() || content.len() > MAX_CONTENT_LEN {
            return Err(ErrorKind::Validation(format!("content must be 1-{MAX_CONTENT_LEN} characters")).into());
        }
        Ok(())
    }

    fn validate_tags(tags: &[String]) -> AppResult<()> {
        if tags.len() > MAX_TAGS {
            return Err(ErrorKind::Validation(format!("at most {MAX_TAGS} tags allowed")).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("tag-{index}")).collect()
    }

    #[test]
    fn prompt_template_create_accepts_boundary_values() {
        let title = "t".repeat(MAX_TITLE_LEN);
        let content = "c".repeat(MAX_CONTENT_LEN);
        assert!(PromptTemplatePolicy::validate_create(&title, &content, &tags(MAX_TAGS)).is_ok());
    }

    #[test]
    fn prompt_template_create_rejects_empty_or_overlong_text() {
        assert!(PromptTemplatePolicy::validate_create("", "content", &[]).is_err());
        assert!(PromptTemplatePolicy::validate_create(&"t".repeat(MAX_TITLE_LEN + 1), "content", &[]).is_err());
        assert!(PromptTemplatePolicy::validate_create("title", "", &[]).is_err());
        assert!(PromptTemplatePolicy::validate_create("title", &"c".repeat(MAX_CONTENT_LEN + 1), &[]).is_err());
    }

    #[test]
    fn prompt_template_create_rejects_too_many_tags() {
        assert!(PromptTemplatePolicy::validate_create("title", "content", &tags(MAX_TAGS + 1)).is_err());
    }

    #[test]
    fn prompt_template_update_allows_partial_updates() {
        assert!(PromptTemplatePolicy::validate_update(None, None, None).is_ok());
        assert!(PromptTemplatePolicy::validate_update(Some("title"), None, None).is_ok());
        assert!(PromptTemplatePolicy::validate_update(None, Some("content"), Some(&tags(MAX_TAGS))).is_ok());
    }

    #[test]
    fn prompt_template_update_validates_present_fields() {
        assert!(PromptTemplatePolicy::validate_update(Some(""), None, None).is_err());
        assert!(PromptTemplatePolicy::validate_update(None, Some(""), None).is_err());
        assert!(PromptTemplatePolicy::validate_update(None, None, Some(&tags(MAX_TAGS + 1))).is_err());
    }
}
