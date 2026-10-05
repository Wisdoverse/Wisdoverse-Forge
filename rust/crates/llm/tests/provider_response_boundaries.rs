//! Local-only checks for provider recipients, response budgets and error privacy.

use agentforge_llm::provider::timed_client;
use agentforge_llm::{
    AnthropicProvider, ChatMessage, ChatRequest, DiscoveryError, GeminiProvider, LlmError, LlmProvider, OpenAiProvider,
    ProviderTransport, discover_models,
};
use serde_json::json;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEST_KEY: &str = "test-only-provider-key";
const RESPONSE_BUDGET: usize = 1024 * 1024;

fn request() -> ChatRequest {
    ChatRequest {
        model: "test-model".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "test-only-private-prompt".into() }],
        max_tokens: Some(16),
        temperature: None,
    }
}

fn providers(base: &str) -> Vec<Box<dyn LlmProvider>> {
    vec![
        Box::new(OpenAiProvider::new(TEST_KEY.into(), Some(base.into()))),
        Box::new(AnthropicProvider::new(TEST_KEY.into(), Some(base.into()))),
        Box::new(GeminiProvider::with_base_url(TEST_KEY.into(), base.into())),
    ]
}

fn assert_status<T>(result: Result<T, LlmError>, expected: u16) {
    match result {
        Err(LlmError::Api { status, message }) => {
            assert_eq!(status, expected);
            assert_eq!(message, "provider request failed");
        }
        _ => panic!("expected a fixed provider-status error"),
    }
}

#[tokio::test]
async fn chat_and_stream_redirects_never_contact_a_second_recipient() {
    let selected = MockServer::start().await;
    let recipient = MockServer::start().await;
    for status in [307, 308] {
        selected.reset().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", recipient.uri()))
            .mount(&selected)
            .await;
        for provider in providers(&selected.uri()) {
            assert_status(provider.chat(request()).await, status);
            assert_status(provider.stream(request()).await, status);
        }
        assert_eq!(selected.received_requests().await.unwrap().len(), 6);
        assert!(recipient.received_requests().await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn chat_and_stream_errors_do_not_retain_upstream_bodies() {
    let selected = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string(format!("test-only-upstream-secret{}", "x".repeat(2 * RESPONSE_BUDGET))),
        )
        .mount(&selected)
        .await;
    for provider in providers(&selected.uri()) {
        assert_status(provider.chat(request()).await, 401);
        assert_status(provider.stream(request()).await, 401);
    }
}

#[tokio::test]
async fn oversized_json_and_gemini_aggregate_are_rejected() {
    let selected = MockServer::start().await;
    for endpoint in ["/v1/chat/completions", "/v1/messages"] {
        Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"padding": "x".repeat(RESPONSE_BUDGET + 1)})))
            .mount(&selected)
            .await;
    }
    let frame =
        format!("data: {}\n\n", json!({"candidates": [{"content": {"parts": [{"text": "x".repeat(32 * 1024)}]}}]}));
    Mock::given(method("POST"))
        .and(path("/v1beta/models/test-model:streamGenerateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(frame.repeat(33)))
        .mount(&selected)
        .await;
    for provider in providers(&selected.uri()) {
        assert!(matches!(
            provider.chat(request()).await,
            Err(LlmError::Parse(message)) if message == "provider response exceeds its size limit"
        ));
    }
}

#[tokio::test]
async fn internal_byo_endpoints_still_return_normal_chat() {
    let selected = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"choices": [{"message": {"content": "ok"}}]})))
        .mount(&selected)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"content": [{"text": "ok"}]})))
        .mount(&selected)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/test-model:streamGenerateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "data: {}\n\n",
            json!({"candidates": [{"content": {"parts": [{"text": "ok"}]}, "finishReason": "STOP"}]})
        )))
        .mount(&selected)
        .await;
    let mut adapters = providers(&selected.uri());
    adapters.push(Box::new(OpenAiProvider::ollama(selected.uri())));
    adapters.push(Box::new(OpenAiProvider::compatible("custom", TEST_KEY.into(), selected.uri())));
    for provider in adapters {
        assert_eq!(provider.chat(request()).await.unwrap().content, "ok");
    }
}

#[tokio::test]
async fn discovery_credentials_never_follow_redirects_or_enter_urls() {
    let selected = MockServer::start().await;
    let recipient = MockServer::start().await;
    let client = timed_client();
    for status in [307, 308] {
        selected.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status).insert_header("Location", recipient.uri()))
            .mount(&selected)
            .await;
        for transport in [ProviderTransport::OpenAi, ProviderTransport::Anthropic, ProviderTransport::Gemini] {
            let result =
                discover_models(&client, transport, &selected.uri(), Some(TEST_KEY), Duration::from_secs(8)).await;
            assert!(matches!(result, Err(DiscoveryError::Status(actual)) if actual == status));
        }
        let requests = selected.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        for request in &requests {
            assert!(request.url.query().is_none());
            assert!(!request.headers.contains_key("referer"));
        }
        assert_eq!(requests[0].headers.get("authorization").unwrap(), &format!("Bearer {TEST_KEY}"));
        assert_eq!(requests[1].headers.get("x-api-key").unwrap(), TEST_KEY);
        assert_eq!(requests[2].headers.get("x-goog-api-key").unwrap(), TEST_KEY);
        assert!(recipient.received_requests().await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn request_errors_do_not_retain_sensitive_urls() {
    let selected = MockServer::start().await;
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(400)).mount(&selected).await;
    let raw = timed_client()
        .get(format!("{}/?key=test-only-query-key", selected.uri()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap_err();
    assert!(raw.url().is_some());
    let error = LlmError::from(raw);
    assert!(!error.to_string().contains("test-only-query-key"));
    assert!(matches!(error, LlmError::Http(error) if error.url().is_none()));
}
