use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use agentforge_orchestrator::state::AppState;

async fn json_response(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = app.oneshot(req).await.expect("request should succeed");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024).await.expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    (status, json)
}

#[tokio::test]
async fn teams_return_service_unavailable_without_store() {
    let app = AppState::test_internal_token("secret-token").router();

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, body) = json_response(app, req).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, serde_json::json!({"ok": false, "error": "database not configured"}));
}

#[tokio::test]
async fn teams_support_create_list_get_update_members_and_delete_round_trip() {
    let app = AppState::test_team_internal_token("secret-token", "org-test", "cli-user").router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/teams")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Backend Team"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["team"]["name"], "Backend Team");
    assert_eq!(created["team"]["orgId"], "org-test");
    assert_eq!(created["team"]["createdBy"], "cli-user");
    let team_id = created["team"]["id"].as_str().expect("team id").to_string();

    let list_req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, listed) = json_response(app.clone(), list_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["teams"].as_array().expect("teams").len(), 1);
    assert_eq!(listed["teams"][0]["id"], team_id);

    let get_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/teams/{team_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched) = json_response(app.clone(), get_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["team"]["id"], team_id);
    assert_eq!(fetched["team"]["members"], serde_json::json!([]));

    let update_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/teams/{team_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Platform Team"}"#))
        .unwrap();
    let (status, updated) = json_response(app.clone(), update_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["team"]["name"], "Platform Team");

    let add_member_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/teams/{team_id}/members"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"participantId":"participant-2"}"#))
        .unwrap();
    let (status, member_created) = json_response(app.clone(), add_member_req).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(member_created["member"]["participantId"], "participant-2");
    assert_eq!(member_created["member"]["role"], "member");

    let get_members_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/teams/{team_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, fetched_with_member) = json_response(app.clone(), get_members_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_with_member["team"]["members"].as_array().expect("members").len(), 1);

    let remove_member_req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/teams/{team_id}/members/participant-2"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, removed) = json_response(app.clone(), remove_member_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(removed, serde_json::json!({"ok": true}));

    let delete_req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/teams/{team_id}"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, deleted) = json_response(app.clone(), delete_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted, serde_json::json!({"ok": true}));

    let list_after_delete_req = Request::builder()
        .method("GET")
        .uri("/api/v1/teams")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .body(Body::empty())
        .unwrap();
    let (status, listed_after_delete) = json_response(app, list_after_delete_req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed_after_delete["teams"], serde_json::json!([]));
}

#[tokio::test]
async fn teams_validate_required_fields() {
    let app = AppState::test_team_internal_token("secret-token", "org-test", "cli-user").router();

    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/teams")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let (status, create_body) = json_response(app.clone(), create_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(create_body, serde_json::json!({"ok": false, "error": "name is required"}));

    let create_team_req = Request::builder()
        .method("POST")
        .uri("/api/v1/teams")
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header("X-User-ID", "cli-user")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"Validation Team"}"#))
        .unwrap();
    let (status, created) = json_response(app.clone(), create_team_req).await;
    assert_eq!(status, StatusCode::CREATED);
    let team_id = created["team"]["id"].as_str().expect("team id");

    let add_member_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/teams/{team_id}/members"))
        .header(header::AUTHORIZATION, "Bearer secret-token")
        .header("X-Org-ID", "org-test")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();
    let (status, member_body) = json_response(app, add_member_req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(member_body, serde_json::json!({"ok": false, "error": "participantId is required"}));
}
