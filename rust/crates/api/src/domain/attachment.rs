//! Attachment domain rules.
//!
//! This module owns upload metadata and quota policies that are independent of
//! repositories, object storage clients, HTTP route DTOs, and persistence details.

use agentforge_core::{AgentId, AppResult, ErrorKind};

const MAX_FILENAME_LEN: usize = 255;
const MAX_CONTENT_TYPE_LEN: usize = 255;

/// Validated attachment filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentFilename {
    value: String,
}

impl AttachmentFilename {
    pub(crate) fn parse(filename: &str) -> AppResult<Self> {
        let filename = filename.trim();
        if filename.is_empty() || filename.len() > MAX_FILENAME_LEN {
            return Err(ErrorKind::Validation(format!("filename must be 1-{MAX_FILENAME_LEN} characters")).into());
        }
        if matches!(filename, "." | "..") {
            return Err(ErrorKind::Validation("filename must not be a relative path segment".into()).into());
        }
        if filename.chars().any(|ch| ch.is_control() || matches!(ch, '/' | '\\')) {
            return Err(ErrorKind::Validation(
                "filename must not contain path separators or control characters".into(),
            )
            .into());
        }
        Ok(Self { value: filename.to_string() })
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

/// Validated attachment content type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentContentType {
    value: String,
}

impl AttachmentContentType {
    pub(crate) fn parse(content_type: &str) -> AppResult<Self> {
        let content_type = content_type.trim();
        if content_type.is_empty() || content_type.len() > MAX_CONTENT_TYPE_LEN {
            return Err(
                ErrorKind::Validation(format!("content_type must be 1-{MAX_CONTENT_TYPE_LEN} characters")).into()
            );
        }
        if content_type.chars().any(char::is_control) {
            return Err(ErrorKind::Validation("content_type must not contain control characters".into()).into());
        }
        Ok(Self { value: content_type.to_string() })
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

/// Validated attachment payload size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachmentPayloadSize {
    bytes: i64,
}

impl AttachmentPayloadSize {
    pub(crate) fn from_len(len: usize, max_file_size: i64) -> AppResult<Self> {
        let bytes = i64::try_from(len).map_err(|_| ErrorKind::Validation("attachment is too large".to_string()))?;
        if bytes > max_file_size {
            return Err(ErrorKind::Validation(format!(
                "attachment exceeds configured size limit of {max_file_size} bytes"
            ))
            .into());
        }
        Ok(Self { bytes })
    }

    pub(crate) fn bytes(self) -> i64 {
        self.bytes
    }
}

/// Attachment count policy for agent-scoped uploads.
pub(crate) struct AttachmentCountPolicy;

impl AttachmentCountPolicy {
    pub(crate) fn ensure_agent_file_slot(
        agent_id: AgentId,
        existing: i64,
        max_files_per_session: i64,
    ) -> AppResult<()> {
        if existing >= max_files_per_session {
            return Err(ErrorKind::Conflict(format!(
                "attachment limit reached for agent {agent_id}: max {max_files_per_session}"
            ))
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_filename_trims_and_accepts_plain_names() {
        let filename = AttachmentFilename::parse(" report.txt ").unwrap();
        assert_eq!(filename.value(), "report.txt");
    }

    #[test]
    fn attachment_filename_rejects_path_segments_and_control_chars() {
        assert!(AttachmentFilename::parse(".").is_err());
        assert!(AttachmentFilename::parse("..").is_err());
        assert!(AttachmentFilename::parse("../report.txt").is_err());
        assert!(AttachmentFilename::parse("nested/report.txt").is_err());
        assert!(AttachmentFilename::parse("bad\nname.txt").is_err());
    }

    #[test]
    fn attachment_filename_rejects_empty_or_overlong_names() {
        assert!(AttachmentFilename::parse("").is_err());
        assert!(AttachmentFilename::parse("   ").is_err());
        assert!(AttachmentFilename::parse(&"a".repeat(MAX_FILENAME_LEN + 1)).is_err());
    }

    #[test]
    fn attachment_content_type_trims_and_rejects_header_injection() {
        let content_type = AttachmentContentType::parse(" text/plain ").unwrap();
        assert_eq!(content_type.value(), "text/plain");
        assert!(AttachmentContentType::parse("text/plain\r\nx: y").is_err());
    }

    #[test]
    fn attachment_payload_size_rejects_configured_limit_overflow() {
        assert_eq!(AttachmentPayloadSize::from_len(10, 10).unwrap().bytes(), 10);
        assert!(AttachmentPayloadSize::from_len(11, 10).is_err());
    }

    #[test]
    fn attachment_count_policy_rejects_full_agent_session() {
        let agent_id = AgentId::new();
        assert!(AttachmentCountPolicy::ensure_agent_file_slot(agent_id, 9, 10).is_ok());
        assert!(AttachmentCountPolicy::ensure_agent_file_slot(agent_id, 10, 10).is_err());
    }
}
