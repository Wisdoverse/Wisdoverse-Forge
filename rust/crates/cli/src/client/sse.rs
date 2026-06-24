use futures::stream::Stream;
use std::pin::Pin;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseEvent {
    pub id: String,
    pub event: String,
    pub data: String,
}

/// Parses raw SSE bytes into events following `cli/internal/client/sse.go:consumeSSE`.
/// Operates on a line iterator (lines without trailing newlines). Emits one
/// `SseEvent` per empty-line boundary; fields default per the SSE spec.
pub fn parse_sse_lines(lines: impl Iterator<Item = String>) -> Vec<SseEvent> {
    let mut out = Vec::new();
    let mut current = SseEvent::default();
    let mut data_lines: Vec<String> = Vec::new();

    for raw in lines {
        if raw.is_empty() {
            if !data_lines.is_empty() {
                current.data = data_lines.join("\n");
                if current.event.is_empty() {
                    current.event = "message".into();
                }
                out.push(std::mem::take(&mut current));
                data_lines.clear();
            }
            continue;
        }
        if raw.starts_with(':') {
            continue; // comment/keepalive
        }
        let (field, value) = match raw.find(':') {
            Some(i) => (&raw[..i], raw[i + 1..].strip_prefix(' ').unwrap_or(&raw[i + 1..])),
            None => (raw.as_str(), ""),
        };
        match field {
            "id" => current.id = value.into(),
            "event" => current.event = value.into(),
            "data" => data_lines.push(value.into()),
            _ => {}
        }
    }
    out
}

/// Live SSE event stream backed by a `reqwest::Response::bytes_stream`.
/// Constructed by [`crate::client::Client::stream_sse`].
pub struct SseStream {
    inner: Pin<Box<dyn Stream<Item = Result<SseEvent, crate::error::CliError>> + Send>>,
}

impl SseStream {
    /// Wraps an arbitrary `Stream<Item = Result<SseEvent, CliError>>` into an `SseStream`.
    /// Used by `Client::stream_sse` to hide the `async_stream::stream!` type behind
    /// a concrete name the rest of the CLI can import.
    pub(crate) fn new<S>(inner: S) -> Self
    where
        S: Stream<Item = Result<SseEvent, crate::error::CliError>> + Send + 'static,
    {
        Self { inner: Box::pin(inner) }
    }
}

impl Stream for SseStream {
    type Item = Result<SseEvent, crate::error::CliError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_event() {
        let lines = vec!["event: tool_start".to_string(), "data: {\"x\":1}".to_string(), String::new()];
        let out = parse_sse_lines(lines.into_iter());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].event, "tool_start");
        assert_eq!(out[0].data, r#"{"x":1}"#);
    }

    #[test]
    fn joins_multiline_data() {
        let lines = vec!["data: line1".to_string(), "data: line2".to_string(), String::new()];
        let out = parse_sse_lines(lines.into_iter());
        assert_eq!(out[0].data, "line1\nline2");
    }

    #[test]
    fn ignores_comment_and_unknown_fields() {
        let lines = vec![
            ": keepalive".to_string(),
            "id: 42".to_string(),
            "event: m".to_string(),
            "data: y".to_string(),
            "retry: 3000".to_string(),
            String::new(),
        ];
        let out = parse_sse_lines(lines.into_iter());
        assert_eq!(out[0].id, "42");
        assert_eq!(out[0].event, "m");
        assert_eq!(out[0].data, "y");
    }

    #[test]
    fn defaults_event_name_to_message() {
        let lines = vec!["data: hello".to_string(), String::new()];
        let out = parse_sse_lines(lines.into_iter());
        assert_eq!(out[0].event, "message");
    }
}
