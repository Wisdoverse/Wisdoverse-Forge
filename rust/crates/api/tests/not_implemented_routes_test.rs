//! #1313: endpoints without a real implementation must not report success.
//! They stay behind auth and return the standard error envelope with 501.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

use agentforge_api::test_support::{seed_provider_agent, test_app_with_mock_provider};

async fn call(app: &axum::Router, method: &str, uri: &str, jwt: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(jwt) = jwt {
        builder = builder.header("authorization", format!("Bearer {jwt}"));
    }
    let response = app.clone().oneshot(builder.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[sqlx::test(migrations = "../db/migrations")]
async fn stub_routes_return_not_implemented_behind_auth(pool: PgPool) {
    let seed = seed_provider_agent(&pool, "openai", "gpt-5.5").await;
    let app = test_app_with_mock_provider(pool.clone(), "openai", "unused").await;

    let agent_git = format!("/api/v1/agents/{}/git", seed.agent_id);
    for (method, uri) in [("POST", "/api/v1/voice/transcribe"), ("GET", agent_git.as_str())] {
        let (status, _) = call(&app, method, uri, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri} must require auth");

        let (status, body) = call(&app, method, uri, Some(&seed.jwt)).await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "{method} {uri}");
        assert_eq!(body["ok"], false, "{method} {uri} must not report success");
        assert_eq!(body["error"]["code"], "NOT_IMPLEMENTED", "{method} {uri}");
    }
}
