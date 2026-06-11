use serde_json::Value;
use std::time::Duration;

pub struct ClientOptions {
    pub server: String,
    pub token: Option<String>,
    pub timeout: Duration,
    pub insecure: bool,
    pub verbose: bool,
    pub debug: bool,
    pub trace: bool,
}

pub struct Client {
    inner: reqwest::Client,
    opts: ClientOptions,
}

/// Caller-provided hint used to disambiguate stages 3 and 4 of the Go
/// `unmarshalSuccess` algorithm. Go uses the `reflect.Kind` of the target
/// variable; in Rust we pass it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    /// Best-effort: Stage 1 / Stage 2 / Stage 4 (merged map).
    /// Use for actions, single-object fetches, and anything that is NOT a list.
    Auto,
    /// Caller expects a JSON array. Activates Stage 3: if the stripped body
    /// has exactly one value that is a JSON array, return that array.
    /// Use for `list` endpoints that respond with `{events:[...], total, limit, offset}`,
    /// `{agents:[...]}`, `{groups:[...]}`, `{workers:[...]}`, etc.
    Array,
}

impl Client {
    pub fn new(opts: ClientOptions) -> anyhow::Result<Self> {
        let builder = reqwest::Client::builder().timeout(opts.timeout);
        Ok(Self { inner: builder.build()?, opts })
    }

    /// Sends the request with retry logic for 429 and idempotent 5xx responses,
    /// and returns the raw parsed body `Value` — before any envelope unwrapping.
    async fn send_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Option<Value>, crate::error::CliError> {
        use crate::error::CliError;

        const MAX_RETRIES_429: u32 = 3;
        const MAX_RETRIES_5XX: u32 = 2;

        let url = format!("{}{}", self.opts.server.trim_end_matches('/'), path);
        let body_bytes: Option<Vec<u8>> = body
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|e| CliError::Other(format!("serialize request body: {e}")))?;
        let is_idempotent = matches!(method, reqwest::Method::GET | reqwest::Method::DELETE);

        let mut attempt_429: u32 = 0;
        let mut attempt_5xx: u32 = 0;

        let (status, bytes) = loop {
            let mut req = self.inner.request(method.clone(), &url);
            if let Some(t) = &self.opts.token {
                req = req.bearer_auth(t);
            }
            if let Some(b) = &body_bytes {
                req = req.header("Content-Type", "application/json").body(b.clone());
            }
            req = req.header("Accept", "application/json");

            // Inject W3C traceparent header when tracing is active.
            if self.opts.trace {
                let mut trace_headers = reqwest::header::HeaderMap::new();
                crate::otel::inject_headers(&mut trace_headers);
                for (name, value) in trace_headers.iter() {
                    req = req.header(name.clone(), value.clone());
                }
            }

            // Verbose: one-line method+path to stderr.
            if self.opts.verbose {
                eprintln!("{method} {path}");
            }

            // Debug: dump method + URL + masked headers + body to stderr.
            if self.opts.debug {
                eprintln!("--- REQUEST ---");
                eprintln!("{method} {url}");
                if self.opts.token.is_some() {
                    eprintln!("Authorization: Bearer ***");
                }
                if body_bytes.is_some() {
                    eprintln!("Content-Type: application/json");
                }
                eprintln!("Accept: application/json");
                if let Some(b) = &body_bytes {
                    eprintln!();
                    eprintln!("{}", String::from_utf8_lossy(b));
                }
            }

            let resp = req.send().await.map_err(|e| CliError::Transport(format!("http {method} {url}: {e}")))?;
            let status = resp.status();

            if self.opts.debug {
                eprintln!("--- RESPONSE ---");
                eprintln!("{}", status);
                for (name, value) in resp.headers() {
                    eprintln!("{}: {}", name, String::from_utf8_lossy(value.as_bytes()));
                }
            }

            if status.as_u16() == 429 {
                if attempt_429 >= MAX_RETRIES_429 {
                    return Err(CliError::Api(crate::client::ApiError {
                        code: "RATE_LIMITED".into(),
                        message: "rate limit exceeded after retries".into(),
                        status: 429,
                    }));
                }
                let retry_after =
                    resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<u64>().ok());
                let delay = retry_after.map(std::time::Duration::from_secs).unwrap_or_else(|| {
                    let base = std::time::Duration::from_secs(1u64 << attempt_429);
                    base + jitter()
                });
                attempt_429 += 1;
                drop(resp);
                tokio::time::sleep(delay).await;
                continue;
            }

            let s = status.as_u16();
            if (s == 502 || s == 503) && is_idempotent {
                if attempt_5xx >= MAX_RETRIES_5XX {
                    return Err(CliError::Api(crate::client::ApiError {
                        code: "SERVICE_UNAVAILABLE".into(),
                        message: "service unavailable after retries".into(),
                        status: s,
                    }));
                }
                let delay = std::time::Duration::from_secs(1u64 << attempt_5xx) + jitter();
                attempt_5xx += 1;
                drop(resp);
                tokio::time::sleep(delay).await;
                continue;
            }

            let bytes = resp.bytes().await.map_err(|e| CliError::Transport(format!("read response body: {e}")))?;
            break (status, bytes);
        };

        if status.is_client_error() || status.is_server_error() {
            if let Ok(env) = serde_json::from_slice::<Value>(&bytes)
                && let Some(code) = env.get("error").and_then(|v| v.as_str())
            {
                let message = env.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                return Err(CliError::Api(crate::client::ApiError {
                    code: code.to_string(),
                    message,
                    status: status.as_u16(),
                }));
            }
            return Err(CliError::Api(crate::client::ApiError {
                code: "HTTP_ERROR".into(),
                message: format!("HTTP {}", status.as_u16()),
                status: status.as_u16(),
            }));
        }

        if bytes.is_empty() {
            return Ok(None);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| CliError::Other(format!("parse response: {e}")))?;
        Ok(Some(v))
    }

    /// Executes an API request. If `body` is Some it is JSON-encoded.
    /// On success, unwraps the response envelope into `Value` using the
    /// multi-stage unwrap order from `cli/internal/client/client.go:unmarshalSuccess`.
    /// Callers that expect an array payload (list endpoints) MUST pass
    /// `ResponseKind::Array` so stage 3 disambiguates `{events:[...], total}`
    /// correctly. All other callers pass `ResponseKind::Auto`.
    ///
    /// Note: for list endpoints that also need pagination metadata, use
    /// [`Client::do_request_list`] instead — it returns both the array AND
    /// the stripped metadata keys (`total`, `limit`, `offset`) in one call.
    pub async fn do_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
        kind: ResponseKind,
    ) -> Result<Option<Value>, crate::error::CliError> {
        let raw = self.send_raw(method, path, body).await?;
        Ok(raw.map(|v| unwrap_success(v, kind)))
    }

    /// Specialised entry point for list endpoints. Unwraps the envelope
    /// without dropping metadata, then extracts the array payload AND the
    /// pagination fields in a single call. Mirrors the Go CLI's behaviour
    /// of `{ok, data:[...], total, limit, offset}` and
    /// `{ok, events:[...], total, limit, offset}` list responses.
    ///
    /// Returns `(items, total, limit, offset)`.
    /// When pagination fields are absent, `total` falls back to `items.len()`
    /// so list commands match the Go CLI's synthetic pagination metadata,
    /// while `limit`/`offset` stay at 0 so callers can apply command defaults.
    pub async fn do_request_list(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(Vec<Value>, u64, u64, u64), crate::error::CliError> {
        use crate::error::CliError;
        let raw = self
            .send_raw(method, path, body)
            .await?
            .ok_or_else(|| CliError::Other("empty response body for list endpoint".into()))?;

        // If the raw body is already an array, there is no sibling pagination metadata.
        if let Value::Array(arr) = raw {
            let n = arr.len() as u64;
            return Ok((arr, n, 0, 0));
        }

        // Otherwise expect `{ok, data:[...], total?, limit?, offset?}` or
        // `{ok, events:[...], total?, limit?, offset?}`. Preserve pagination
        // from the original envelope before unwrapping the payload array.
        let Value::Object(mut obj) = raw else {
            return Ok((Vec::new(), 0, 0, 0));
        };

        let total = obj.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let limit = obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(0);
        let offset = obj.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);

        obj.remove("ok");
        obj.remove("error");
        obj.remove("message");

        // Prefer `data`, else the single remaining array key.
        let items = if let Some(Value::Array(arr)) = obj.remove("data") {
            arr
        } else {
            let array_key = obj.iter().find(|(_, v)| matches!(v, Value::Array(_))).map(|(k, _)| k.clone());
            match array_key.and_then(|k| obj.remove(&k)) {
                Some(Value::Array(arr)) => arr,
                _ => Vec::new(),
            }
        };

        let total = if total > 0 { total } else { items.len() as u64 };

        Ok((items, total, limit, offset))
    }
}

/// Mirrors the 5-stage unwrap in cli/internal/client/client.go:unmarshalSuccess.
///
/// Stages:
///   1. `{ok, data: ...}` → `data`
///   2. strip `ok`/`error`/`message` → exactly one remaining key → that key's value
///   3. `kind == Array` AND exactly one stripped value is an array → that array
///      (matches Go's slice-kind branch driven by `singleJSONArray`)
///   4. otherwise → the stripped map merged as Value::Object
///      (matches Go's map/struct-kind branch for metadata payloads like
///      `{total, dispatched, results}` returned by `groups dispatch`)
///   5. scalar / top-level array → return as-is
fn unwrap_success(v: Value, kind: ResponseKind) -> Value {
    // Stage 1: {ok, data: ...} → data
    if let Some(obj) = v.as_object()
        && let Some(data) = obj.get("data")
    {
        return data.clone();
    }

    if let Value::Object(mut obj) = v.clone() {
        obj.remove("ok");
        obj.remove("error");
        obj.remove("message");

        // Stage 2: exactly one key left → that key's value.
        if obj.len() == 1 {
            return obj.into_iter().next().unwrap().1;
        }

        // Stage 3 (array hint): exactly one remaining value is an array → it.
        if kind == ResponseKind::Array {
            let arrays: Vec<&Value> = obj.values().filter(|v| matches!(v, Value::Array(_))).collect();
            if arrays.len() == 1 {
                return arrays[0].clone();
            }
        }

        // Stage 4: stripped map (metadata / mixed payload).
        return Value::Object(obj);
    }

    // Stage 5: scalar or top-level array — return as-is.
    v
}

impl Client {
    /// Opens an SSE stream with reconnect + `Last-Event-ID` header.
    /// Matches `cli/internal/client/sse.go:StreamSSE`.
    ///
    /// `max_retries == 0` means unlimited reconnect attempts. Otherwise,
    /// after `max_retries` consecutive connection failures, the stream
    /// yields a single `CliError::Stream` and terminates.
    pub async fn stream_sse(
        &self,
        path: &str,
        max_retries: u32,
    ) -> Result<crate::client::sse::SseStream, crate::error::CliError> {
        use crate::error::CliError;
        use futures::StreamExt;

        let url = format!("{}{}", self.opts.server.trim_end_matches('/'), path);
        let token = self.opts.token.clone();
        let verbose = self.opts.verbose;
        let debug = self.opts.debug;
        let trace = self.opts.trace;
        let path_str = path.to_string();
        // SSE streams are long-lived; build a separate client without the
        // request timeout so a 30s `opts.timeout` doesn't kill the stream.
        // Matches `cli/internal/client/sse.go:consumeSSE` which uses
        // `&http.Client{Transport: c.http.Transport}` with no Timeout.
        let client =
            reqwest::Client::builder().build().map_err(|e| CliError::Other(format!("build sse client: {e}")))?;

        let stream = async_stream::stream! {
            let mut last_event_id = String::new();
            let mut retries: u32 = 0;
            let mut backoff = std::time::Duration::from_secs(1);

            loop {
                let mut req = client
                    .get(&url)
                    .header("Accept", "text/event-stream")
                    .header("Cache-Control", "no-cache");
                if let Some(t) = &token {
                    req = req.bearer_auth(t);
                }
                if !last_event_id.is_empty() {
                    req = req.header("Last-Event-ID", &last_event_id);
                }

                // Inject W3C traceparent header when tracing is active.
                if trace {
                    let mut trace_headers = reqwest::header::HeaderMap::new();
                    crate::otel::inject_headers(&mut trace_headers);
                    for (name, value) in trace_headers.iter() {
                        req = req.header(name.clone(), value.clone());
                    }
                }

                if verbose {
                    eprintln!("GET {path_str} (SSE)");
                }
                if debug {
                    eprintln!("--- SSE REQUEST ---");
                    eprintln!("GET {url}");
                    if token.is_some() {
                        eprintln!("Authorization: Bearer ***");
                    }
                    eprintln!("Accept: text/event-stream");
                    eprintln!("Cache-Control: no-cache");
                    if !last_event_id.is_empty() {
                        eprintln!("Last-Event-ID: {last_event_id}");
                    }
                }

                match req.send().await {
                    Err(e) => {
                        retries += 1;
                        if max_retries > 0 && retries > max_retries {
                            yield Err(CliError::Stream(format!(
                                "SSE connection failed after {max_retries} retries: {e}"
                            )));
                            return;
                        }
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(std::time::Duration::from_secs(10));
                        continue;
                    }
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            yield Err(CliError::Api(crate::client::ApiError {
                                code: format!("HTTP_{}", resp.status().as_u16()),
                                message: format!("SSE endpoint returned {}", resp.status().as_u16()),
                                status: resp.status().as_u16(),
                            }));
                            return;
                        }
                        let ct = resp
                            .headers()
                            .get("content-type")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if !ct.starts_with("text/event-stream") {
                            yield Err(CliError::Stream(format!(
                                "unexpected Content-Type: {ct} (expected text/event-stream)"
                            )));
                            return;
                        }

                        let mut body = resp.bytes_stream();
                        let mut buf = Vec::<u8>::new();
                        let mut data_lines: Vec<String> = Vec::new();
                        let mut current = crate::client::sse::SseEvent::default();

                        'conn: while let Some(chunk) = body.next().await {
                            let chunk = match chunk {
                                Ok(b) => b,
                                Err(_) => break 'conn,
                            };
                            buf.extend_from_slice(&chunk);
                            while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                                let line_bytes: Vec<u8> = buf.drain(..=nl).collect();
                                let mut line = String::from_utf8_lossy(&line_bytes).to_string();
                                if line.ends_with('\n') { line.pop(); }
                                if line.ends_with('\r') { line.pop(); }

                                if line.is_empty() {
                                    if !data_lines.is_empty() {
                                        current.data = data_lines.join("\n");
                                        if current.event.is_empty() {
                                            current.event = "message".into();
                                        }
                                        if !current.id.is_empty() {
                                            last_event_id = current.id.clone();
                                        }
                                        yield Ok(std::mem::take(&mut current));
                                        data_lines.clear();
                                    }
                                    continue;
                                }
                                if line.starts_with(':') { continue; }
                                let (field, value) = match line.find(':') {
                                    Some(i) => {
                                        let (f, v) = line.split_at(i);
                                        (f.to_string(), v[1..].strip_prefix(' ').unwrap_or(&v[1..]).to_string())
                                    }
                                    None => (line.clone(), String::new()),
                                };
                                match field.as_str() {
                                    "id" => current.id = value,
                                    "event" => current.event = value,
                                    "data" => data_lines.push(value),
                                    _ => {}
                                }
                            }
                        }
                        retries = 0;
                        backoff = std::time::Duration::from_secs(1);
                        // reconnect after server closes
                    }
                }
            }
        };

        Ok(crate::client::sse::SseStream::new(stream))
    }
}

fn jitter() -> std::time::Duration {
    use rand::RngExt;
    let ms = rand::rng().random_range(0..500);
    std::time::Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn unwraps_envelope_data_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/agents/x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "data": { "id": "x", "name": "a" }
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let out = client.do_request(reqwest::Method::GET, "/api/v1/agents/x", None, ResponseKind::Auto).await.unwrap();
        assert_eq!(out.unwrap(), json!({"id":"x","name":"a"}));
    }

    #[tokio::test]
    async fn unwraps_single_named_payload_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/agents/x"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "agent": { "id": "x" }
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let out = client.do_request(reqwest::Method::GET, "/api/v1/agents/x", None, ResponseKind::Auto).await.unwrap();
        assert_eq!(out.unwrap(), json!({"id":"x"}));
    }

    #[tokio::test]
    async fn unwraps_named_array_with_metadata_when_array_kind() {
        // Mirrors `{ok, events:[...], total, limit, offset}` shape returned by list endpoints.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "events": [{"id":"e1"},{"id":"e2"}],
                "total": 2, "limit": 50, "offset": 0
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let out = client.do_request(reqwest::Method::GET, "/api/v1/events", None, ResponseKind::Array).await.unwrap();
        assert_eq!(out.unwrap(), json!([{"id":"e1"},{"id":"e2"}]));
    }

    #[tokio::test]
    async fn preserves_pagination_metadata_for_data_envelopes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/agents"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "data": [{"id":"a"},{"id":"b"}],
                "total": 42,
                "limit": 50,
                "offset": 100
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let (items, total, limit, offset) =
            client.do_request_list(reqwest::Method::GET, "/api/v1/agents", None).await.unwrap();

        assert_eq!(items, vec![json!({"id":"a"}), json!({"id":"b"})]);
        assert_eq!(total, 42);
        assert_eq!(limit, 50);
        assert_eq!(offset, 100);
    }

    #[tokio::test]
    async fn synthesizes_total_for_enveloped_arrays_without_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/groups"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "data": [{"id":"g1"},{"id":"g2"}]
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let (items, total, limit, offset) =
            client.do_request_list(reqwest::Method::GET, "/api/v1/groups", None).await.unwrap();

        assert_eq!(items, vec![json!({"id":"g1"}), json!({"id":"g2"})]);
        assert_eq!(total, 2);
        assert_eq!(limit, 0);
        assert_eq!(offset, 0);
    }

    #[tokio::test]
    async fn preserves_metadata_map_when_auto_kind() {
        // `groups dispatch` returns `{ok, total, dispatched, failed, results}` and
        // the caller needs the WHOLE map — Stage 3 must NOT fire without Array hint.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/groups/g/dispatch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true,
                "total": 3, "dispatched": 2, "failed": 1,
                "results": [{"agentId":"a"}]
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let out = client
            .do_request(reqwest::Method::POST, "/api/v1/groups/g/dispatch", Some(&json!({})), ResponseKind::Auto)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out.get("total"), Some(&json!(3)));
        assert_eq!(out.get("dispatched"), Some(&json!(2)));
        assert_eq!(out.get("failed"), Some(&json!(1)));
        assert!(out.get("results").is_some());
    }

    #[tokio::test]
    async fn retries_429_up_to_three_times_then_errors() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};
        let server = MockServer::start().await;
        // Four 429s: three retries + initial attempt = RATE_LIMITED
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .expect(4)
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: std::time::Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let err = client.do_request(reqwest::Method::GET, "/x", None, ResponseKind::Auto).await.unwrap_err();
        match err {
            crate::error::CliError::Api(api) if api.code == "RATE_LIMITED" => {}
            other => panic!("expected RATE_LIMITED, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn does_not_retry_5xx_on_post() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: std::time::Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let _ = client.do_request(reqwest::Method::POST, "/x", None, ResponseKind::Auto).await.unwrap_err();
    }

    #[tokio::test]
    async fn retries_429_then_succeeds_on_second_attempt() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // First request: 429. Second request: 200 with a valid envelope.
        // wiremock's `up_to_n_times(1)` makes the first mock only match the first request;
        // subsequent matching requests fall through to the next mock.
        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "data": { "id": "success" }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: std::time::Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let out = client
            .do_request(reqwest::Method::GET, "/ok", None, ResponseKind::Auto)
            .await
            .expect("retry should succeed on second attempt");

        assert_eq!(out.unwrap(), json!({"id":"success"}));
    }

    #[tokio::test]
    async fn unwraps_error_envelope_to_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/agents/x"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "ok": false, "error": "NOT_FOUND", "message": "agent x not found"
            })))
            .mount(&server)
            .await;

        let client = Client::new(ClientOptions {
            server: server.uri(),
            token: None,
            timeout: Duration::from_secs(5),
            insecure: false,
            verbose: false,
            debug: false,
            trace: false,
        })
        .unwrap();

        let err =
            client.do_request(reqwest::Method::GET, "/api/v1/agents/x", None, ResponseKind::Auto).await.unwrap_err();
        match err {
            crate::error::CliError::Api(api) => {
                assert_eq!(api.code, "NOT_FOUND");
                assert_eq!(api.status, 404);
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }
}
