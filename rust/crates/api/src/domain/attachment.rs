//! Attachment domain rules.
//!
//! This module owns upload metadata and quota policies that are independent of
//! repositories, object storage clients, HTTP route DTOs, and persistence details.

use std::io::Cursor;

use agentforge_core::{AgentId, AppError, AppResult, ErrorKind};
use image::{ImageFormat, ImageReader};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

const MAX_FILENAME_LEN: usize = 255;
const MAX_CONTENT_TYPE_LEN: usize = 255;
pub(crate) const DEFAULT_ATTACHMENT_CONTENT_TYPE: &str = "application/octet-stream";

/// Hard ceiling on decoded image pixels (≈50 MP) to bound memory before a full
/// decode, defeating decompression bombs. Checked from the header only.
pub(crate) const MAX_IMAGE_PIXELS: u64 = 50_000_000;

/// Canonical content type for re-encoded instruction images.
pub(crate) const IMAGE_OUTPUT_CONTENT_TYPE: &str = "image/png";

/// A validated, re-encoded image ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedImage {
    pub(crate) png_bytes: Vec<u8>,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

/// Validate untrusted image bytes and re-encode to canonical PNG.
///
/// Security: this single decode→re-encode pass is the trust boundary for image
/// input that a `--dangerously-skip-permissions` CLI agent may later read.
/// 1. Format is detected from MAGIC BYTES (not the declared content type) and
///    must be in the allowlist (png/jpeg/webp/gif).
/// 2. Dimensions are read from the header BEFORE decoding; an image whose pixel
///    count exceeds `max_pixels` is rejected without allocating the canvas
///    (decompression-bomb guard).
/// 3. The decoded image is re-encoded to PNG, which drops EXIF/GPS metadata and
///    any trailing/appended polyglot payload (bytes after the image data).
pub(crate) fn validate_and_reencode_image(bytes: &[u8], max_pixels: u64) -> AppResult<ValidatedImage> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ErrorKind::Validation(format!("could not read image: {e}")))?;
    let format = reader.format().ok_or_else(|| ErrorKind::Validation("unrecognized image format".to_string()))?;
    if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif) {
        return Err(ErrorKind::Validation(format!("unsupported image format: {format:?}")).into());
    }

    let (width, height) =
        reader.into_dimensions().map_err(|e| ErrorKind::Validation(format!("could not read image dimensions: {e}")))?;
    if u64::from(width) * u64::from(height) > max_pixels {
        return Err(
            ErrorKind::Validation(format!("image {width}x{height} exceeds the {max_pixels}-pixel limit")).into()
        );
    }

    let decoded = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ErrorKind::Validation(format!("could not read image: {e}")))?
        .decode()
        .map_err(|e| ErrorKind::Validation(format!("could not decode image: {e}")))?;
    let mut png_bytes = Vec::new();
    decoded
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .map_err(|e| AppError::from(ErrorKind::Internal(anyhow::anyhow!("png re-encode failed: {e}"))))?;

    Ok(ValidatedImage {
        png_bytes,
        width: i32::try_from(decoded.width())
            .map_err(|_| ErrorKind::Validation("image width too large".to_string()))?,
        height: i32::try_from(decoded.height())
            .map_err(|_| ErrorKind::Validation("image height too large".to_string()))?,
    })
}

/// Normalize an upload filename to a `.png` name (images are re-encoded to PNG).
/// The result is still validated by `AttachmentFilename::parse`, so a stem with
/// path separators is rejected downstream.
pub(crate) fn image_output_filename(original: &str) -> String {
    let stem = original.rsplit_once('.').map(|(stem, _ext)| stem).unwrap_or(original).trim();
    let stem = if stem.is_empty() { "image" } else { stem };
    format!("{stem}.png")
}

pub(crate) fn attachment_data_response<T: Serialize>(data: T) -> Value {
    json!({ "ok": true, "data": data })
}

pub(crate) fn attachment_delete_response() -> Value {
    json!({ "ok": true })
}

pub(crate) struct AttachmentRepositoryPolicy;

impl AttachmentRepositoryPolicy {
    pub(crate) fn attachment_not_found(id: Uuid) -> AppError {
        ErrorKind::NotFound(format!("attachment {id}")).into()
    }
}

/// Upload payload after HTTP multipart fields have been read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentUploadDraft {
    pub(crate) filename: String,
    pub(crate) content_type: String,
    pub(crate) agent_id: Option<AgentId>,
    pub(crate) bytes: Vec<u8>,
}

impl AttachmentUploadDraft {
    pub(crate) fn from_parts(
        file_name: Option<String>,
        file_content_type: Option<String>,
        filename_override: Option<String>,
        content_type_override: Option<String>,
        agent_id: Option<AgentId>,
        bytes: Option<Vec<u8>>,
    ) -> AppResult<Self> {
        let bytes = bytes.ok_or_else(|| ErrorKind::Validation("multipart field 'file' is required".to_string()))?;
        let filename = filename_override
            .or(file_name)
            .ok_or_else(|| ErrorKind::Validation("attachment filename is required".to_string()))?;
        let content_type =
            content_type_override.or(file_content_type).unwrap_or_else(|| DEFAULT_ATTACHMENT_CONTENT_TYPE.to_string());

        Ok(Self { filename, content_type, agent_id, bytes })
    }
}

/// Attachment body and metadata prepared for a download response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachmentDownload {
    filename: String,
    content_type: String,
    bytes: Vec<u8>,
}

impl AttachmentDownload {
    pub(crate) fn new(filename: String, content_type: String, bytes: Vec<u8>) -> Self {
        Self { filename, content_type, bytes }
    }

    pub(crate) fn filename(&self) -> &str {
        &self.filename
    }

    pub(crate) fn content_type(&self) -> &str {
        &self.content_type
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub(crate) fn attachment_download_content_disposition(filename: &str) -> String {
    let escaped = filename
        .chars()
        .map(|ch| match ch {
            '"' | '\\' | '\r' | '\n' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect::<String>();
    format!("attachment; filename=\"{escaped}\"")
}

/// Multipart upload error policy for route-level HTTP field extraction.
pub(crate) struct AttachmentMultipartPolicy;

impl AttachmentMultipartPolicy {
    pub(crate) fn missing_field_name() -> ErrorKind {
        ErrorKind::Validation("multipart field name is required".to_string())
    }

    pub(crate) fn duplicate_file_field() -> ErrorKind {
        ErrorKind::Validation("exactly one file field is allowed".to_string())
    }

    pub(crate) fn unsupported_field(name: &str) -> ErrorKind {
        ErrorKind::Validation(format!("unsupported multipart field '{name}'"))
    }

    pub(crate) fn invalid_body(err: impl std::fmt::Display) -> ErrorKind {
        ErrorKind::Validation(format!("invalid multipart body: {err}"))
    }
}

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

/// Agent association supplied on attachment upload.
pub(crate) struct AttachmentAgentScope;

impl AttachmentAgentScope {
    pub(crate) fn parse(value: &str) -> AppResult<AgentId> {
        let trimmed = value.trim();
        let id = Uuid::parse_str(trimmed).map_err(|_| ErrorKind::Validation("agent_id must be a UUID".to_string()))?;
        Ok(AgentId::from(id))
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

    #[test]
    fn attachment_agent_scope_trims_and_parses_uuid() {
        let parsed = AttachmentAgentScope::parse(" 550e8400-e29b-41d4-a716-446655440000 ").unwrap();

        assert_eq!(parsed.as_uuid().to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn attachment_agent_scope_rejects_non_uuid() {
        assert!(AttachmentAgentScope::parse("not-a-uuid").is_err());
    }

    #[test]
    fn upload_draft_prefers_overrides_and_defaults_content_type() {
        let agent_id = AgentId::new();
        let draft = AttachmentUploadDraft::from_parts(
            Some("source.txt".to_string()),
            None,
            Some("override.txt".to_string()),
            None,
            Some(agent_id),
            Some(vec![1, 2, 3]),
        )
        .unwrap();

        assert_eq!(draft.filename, "override.txt");
        assert_eq!(draft.content_type, DEFAULT_ATTACHMENT_CONTENT_TYPE);
        assert_eq!(draft.agent_id, Some(agent_id));
        assert_eq!(draft.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn upload_draft_requires_file_and_filename() {
        assert!(AttachmentUploadDraft::from_parts(None, None, None, None, None, None).is_err());
        assert!(AttachmentUploadDraft::from_parts(None, None, None, None, None, Some(vec![1])).is_err());
    }

    #[test]
    fn download_content_disposition_escapes_unsafe_characters() {
        assert_eq!(
            attachment_download_content_disposition("bad\"\r\nname.txt"),
            "attachment; filename=\"bad___name.txt\""
        );
    }

    #[test]
    fn multipart_policy_owns_upload_field_errors() {
        assert!(format!("{}", AttachmentMultipartPolicy::missing_field_name()).contains("field name"));
        assert!(format!("{}", AttachmentMultipartPolicy::duplicate_file_field()).contains("one file"));
        assert!(format!("{}", AttachmentMultipartPolicy::unsupported_field("debug")).contains("debug"));
        assert!(format!("{}", AttachmentMultipartPolicy::invalid_body("bad boundary")).contains("invalid multipart"));
    }

    #[test]
    fn attachment_repository_policy_owns_lookup_error() {
        let id = Uuid::new_v4();

        assert!(matches!(
            AttachmentRepositoryPolicy::attachment_not_found(id).kind,
            ErrorKind::NotFound(message) if message == format!("attachment {id}")
        ));
    }

    fn tiny_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([10, 20, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .expect("encode test png");
        bytes
    }

    #[test]
    fn validate_image_accepts_and_reencodes_png() {
        let png = tiny_png(4, 3);
        let validated = validate_and_reencode_image(&png, MAX_IMAGE_PIXELS).expect("valid png");

        assert_eq!(validated.width, 4);
        assert_eq!(validated.height, 3);
        assert!(validated.png_bytes.starts_with(&[0x89, b'P', b'N', b'G']), "output must be PNG");
    }

    #[test]
    fn validate_image_rejects_non_image_bytes() {
        assert!(validate_and_reencode_image(b"definitely not an image at all", MAX_IMAGE_PIXELS).is_err());
    }

    #[test]
    fn validate_image_rejects_dimensions_over_ceiling() {
        let png = tiny_png(100, 100); // 10_000 px
        let err = validate_and_reencode_image(&png, 9_999).expect_err("over-ceiling image must be rejected");
        assert!(matches!(err.kind, ErrorKind::Validation(msg) if msg.contains("pixel")));
    }

    #[test]
    fn image_output_filename_normalizes_to_png() {
        assert_eq!(image_output_filename("screenshot"), "screenshot.png");
        assert_eq!(image_output_filename("photo.jpeg"), "photo.png");
        assert_eq!(image_output_filename("a.b.webp"), "a.b.png");
        assert_eq!(image_output_filename(".hidden"), "image.png");
        assert_eq!(image_output_filename(""), "image.png");
    }

    #[test]
    fn validate_image_strips_trailing_polyglot_payload() {
        let mut png = tiny_png(4, 4);
        png.extend_from_slice(b"<<<MALICIOUS_SCRIPT_PAYLOAD>>>");

        let validated = validate_and_reencode_image(&png, MAX_IMAGE_PIXELS).expect("valid png prefix");

        let needle = b"MALICIOUS_SCRIPT_PAYLOAD";
        assert!(
            !validated.png_bytes.windows(needle.len()).any(|w| w == needle),
            "re-encode must drop appended polyglot bytes"
        );
    }
}
