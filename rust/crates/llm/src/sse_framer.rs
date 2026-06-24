//! Shared SSE framer: byte stream → stream of `data:` payloads.
//!
//! Fixes the UTF-8 corruption bug that afflicted the per-provider parsers:
//! `String::from_utf8_lossy` on each raw chunk replaces half-codepoints with
//! U+FFFD when a multi-byte character spans chunk boundaries. This framer
//! buffers bytes, splits on the ASCII-safe `\n\n` frame terminator, and only
//! decodes each complete frame — where by construction the terminator is at
//! a codepoint boundary.
//!
//! Provider-specific JSON interpretation lives in each `{anthropic,openai,gemini}.rs`
//! — this module only carves frames and extracts `data:` payload lines.

use bytes::Bytes;
use futures::stream::{self, Stream, StreamExt};

use crate::provider::LlmError;

/// Yield each `data: <payload>` line (stripped, trimmed) from an SSE byte
/// stream, buffering across chunk boundaries. Non-`data:` SSE lines
/// (`event:`, `retry:`, comments starting with `:`) are discarded — all
/// three LLM providers today rely only on `data:` lines.
pub fn sse_data_payloads(
    bytes: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<String, LlmError>> + Send + 'static {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);

    bytes.flat_map(move |chunk_result| {
        let mut out: Vec<Result<String, LlmError>> = Vec::new();
        match chunk_result {
            Err(e) => out.push(Err(LlmError::Http(e))),
            Ok(chunk) => {
                buf.extend_from_slice(&chunk);
                // Carve complete frames by finding `\n\n` at byte level.
                // `\n` is U+000A / ASCII 0x0A, so this boundary is always
                // at a UTF-8 codepoint boundary regardless of chunk split.
                while let Some(pos) = find_double_newline(&buf) {
                    let frame_bytes: Vec<u8> = buf.drain(..pos + 2).collect();
                    // Now safe to decode — the frame ends at an ASCII boundary.
                    let frame = String::from_utf8_lossy(&frame_bytes);
                    for line in frame.lines() {
                        if let Some(payload) = line.strip_prefix("data: ") {
                            out.push(Ok(payload.trim().to_string()));
                        } else if let Some(payload) = line.strip_prefix("data:") {
                            // Some servers omit the space after the colon.
                            out.push(Ok(payload.trim().to_string()));
                        }
                    }
                }
            }
        }
        stream::iter(out)
    })
}

/// Find the first `\n\n` byte pair in `buf`. Returns index of the first `\n`.
fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;

    fn mk_stream(chunks: Vec<Vec<u8>>) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Send {
        stream::iter(chunks.into_iter().map(|c| Ok::<_, reqwest::Error>(Bytes::from(c))))
    }

    #[tokio::test]
    async fn parses_single_frame() {
        let chunks = vec![b"data: {\"a\":1}\n\n".to_vec()];
        let payloads: Vec<_> = sse_data_payloads(mk_stream(chunks)).map(|r| r.unwrap()).collect().await;
        assert_eq!(payloads, vec!["{\"a\":1}".to_string()]);
    }

    #[tokio::test]
    async fn splits_multiple_frames() {
        let chunks = vec![b"data: 1\n\ndata: 2\n\ndata: 3\n\n".to_vec()];
        let payloads: Vec<_> = sse_data_payloads(mk_stream(chunks)).map(|r| r.unwrap()).collect().await;
        assert_eq!(payloads, vec!["1", "2", "3"]);
    }

    #[tokio::test]
    async fn buffers_across_chunk_boundary() {
        // Split "data: hello\n\n" mid-payload into two chunks.
        let chunks = vec![b"data: hel".to_vec(), b"lo\n\n".to_vec()];
        let payloads: Vec<_> = sse_data_payloads(mk_stream(chunks)).map(|r| r.unwrap()).collect().await;
        assert_eq!(payloads, vec!["hello".to_string()]);
    }

    #[tokio::test]
    async fn preserves_utf8_across_multibyte_chunk_split() {
        // "你" = 0xE4 0xBD 0xA0. Split the 3-byte codepoint across chunks.
        let mut before = b"data: ".to_vec();
        before.push(0xE4);
        before.push(0xBD);
        let mut after = vec![0xA0];
        after.extend_from_slice(b"\n\n");
        let payloads: Vec<_> = sse_data_payloads(mk_stream(vec![before, after])).map(|r| r.unwrap()).collect().await;
        assert_eq!(payloads, vec!["你".to_string()]);
    }

    #[tokio::test]
    async fn ignores_non_data_lines() {
        let chunks = vec![b": comment\nevent: delta\ndata: payload\nretry: 3\n\n".to_vec()];
        let payloads: Vec<_> = sse_data_payloads(mk_stream(chunks)).map(|r| r.unwrap()).collect().await;
        assert_eq!(payloads, vec!["payload".to_string()]);
    }

    #[tokio::test]
    async fn handles_data_prefix_without_space() {
        let chunks = vec![b"data:[DONE]\n\n".to_vec()];
        let payloads: Vec<_> = sse_data_payloads(mk_stream(chunks)).map(|r| r.unwrap()).collect().await;
        assert_eq!(payloads, vec!["[DONE]".to_string()]);
    }
}
