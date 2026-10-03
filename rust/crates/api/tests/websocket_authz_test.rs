//! WebSocket authorization uses live membership before forwarding and after upgrade.

use agentforge_api::test_support::app_state_with_mock_provider;
use sqlx::PgPool;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

#[sqlx::test(migrations = "../db/migrations")]
async fn revoked_websocket_reconnect_is_denied_and_open_connection_closes(pool: PgPool) {
    let org = Uuid::new_v4();
    let user = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, 'Test', $2)")
        .bind(org)
        .bind(format!("ws-{org}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, 'dev@example.com')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO organization_members (organization_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let state = app_state_with_mock_provider(pool.clone(), "mock", "hello").await;
    let token = state.jwt.create_token(user, org, "owner").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, agentforge_api::router::create_router(state)).await.unwrap();
    });
    let client = reqwest::Client::new();
    let upgrade = || {
        client
            .get(format!("http://{address}/ws?token={token}"))
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
    };
    let response = upgrade().send().await.unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::SWITCHING_PROTOCOLS);
    let mut connection = response.upgrade().await.unwrap();
    // Wait for the initial degraded-NATS frame, proving the upgraded handler
    // has finished its initial authorization check before withdrawing access.
    let mut initial = [0u8; 1024];
    let received =
        tokio::time::timeout(std::time::Duration::from_secs(5), connection.read(&mut initial)).await.unwrap().unwrap();
    assert!(received > 0);
    sqlx::query("DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2")
        .bind(org)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let denied = upgrade().send().await.unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::UNAUTHORIZED);
    let mut remaining = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(32), connection.read_to_end(&mut remaining))
        .await
        .expect("withdrawn membership must close an established WebSocket")
        .unwrap();
    server.abort();
}
