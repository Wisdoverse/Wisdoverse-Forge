use std::time::Duration;

use axum::http::{StatusCode, header};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use agentforge_orchestrator::realtime::Event;
use agentforge_orchestrator::state::AppState;

const VALID_SIGNING_KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

async fn spawn_app(app: axum::Router) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve app");
    });
    (format!("ws://{addr}"), handle)
}

#[tokio::test]
async fn websocket_route_requires_authorization_header() {
    let (base_url, handle) = spawn_app(AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY).router()).await;
    let err =
        connect_async(format!("{base_url}/ws/events")).await.expect_err("websocket handshake should fail without auth");

    match err {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        other => panic!("unexpected websocket error: {other:?}"),
    }

    handle.abort();
    let _ = handle.await;
}

#[tokio::test]
async fn websocket_upgrades_for_session_token_and_receives_org_scoped_event() {
    let state = AppState::test_with_jwt_signing_key(VALID_SIGNING_KEY);
    let token_pair = state
        .sessions
        .as_ref()
        .expect("session manager")
        .issue_token_pair("user-1", "user@example.com", "User Example", "org-1")
        .await
        .expect("issue token pair");
    let broadcaster = state.broadcaster.clone();
    let (base_url, handle) = spawn_app(state.router()).await;

    let mut request = format!("{base_url}/ws/events").into_client_request().expect("client request");
    request.headers_mut().insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(&format!("Bearer {}", token_pair.access_token)).expect("auth header"),
    );

    let (mut socket, response) = connect_async(request).await.expect("connect websocket");
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(broadcaster.client_count(), 1);

    broadcaster.broadcast(Event {
        kind: "session_end".to_string(),
        org_id: "org-1".to_string(),
        payload: json!({"sessionId": "abc"}),
    });

    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("message timeout")
        .expect("websocket message")
        .expect("websocket frame");

    match message {
        Message::Text(text) => {
            let value: Value = serde_json::from_str(&text).expect("event json");
            assert_eq!(value["type"], "session_end");
            assert_eq!(value["orgId"], "org-1");
            assert_eq!(value["payload"]["sessionId"], "abc");
        }
        other => panic!("unexpected websocket frame: {other:?}"),
    }

    socket.close(None).await.expect("close socket");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(broadcaster.client_count(), 0);

    handle.abort();
    let _ = handle.await;
}
