use std::sync::Arc;

use agentforge_auth::{AuthUser, JwtManager};
use agentforge_core::{OrgId, ProjectId, TeamId, UserId, WorkspaceId};
use axum::extract::FromRequestParts;
use axum::http::{HeaderValue, Request, header::AUTHORIZATION, request::Parts};
use uuid::Uuid;

const TEST_SECRET: &str = "tenant-scope-axes-test-secret-32-bytes!!";

fn build_parts(token: &str, jwt: Arc<JwtManager>) -> Parts {
    let mut req = Request::builder()
        .uri("/test")
        .header(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}")).unwrap())
        .body(())
        .unwrap();
    req.extensions_mut().insert(jwt);
    let (parts, _body) = req.into_parts();
    parts
}

#[tokio::test]
async fn jwt_scope_axes_populate_tenant_scope() {
    let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
    let user_id = Uuid::now_v7();
    let org_id = Uuid::now_v7();
    let workspace_id = Uuid::now_v7();
    let team_id = Uuid::now_v7();
    let project_id = Uuid::now_v7();
    let token = jwt
        .create_token_with_axes(user_id, org_id, "member", Some(workspace_id), Some(team_id), Some(project_id))
        .unwrap();

    let mut parts = build_parts(&token, jwt);
    let auth = AuthUser::from_request_parts(&mut parts, &()).await.unwrap();

    assert_eq!(auth.scope.org_id(), OrgId::from(org_id));
    assert_eq!(auth.scope.user_id(), UserId::from(user_id));
    assert_eq!(auth.scope.workspace_id(), Some(WorkspaceId::from(workspace_id)));
    assert_eq!(auth.scope.team_id(), Some(TeamId::from(team_id)));
    assert_eq!(auth.scope.project_id(), Some(ProjectId::from(project_id)));
}

#[tokio::test]
async fn missing_team_axis_remains_none_in_tenant_scope() {
    let jwt = Arc::new(JwtManager::new(TEST_SECRET, 3600));
    let workspace_id = Uuid::now_v7();
    let project_id = Uuid::now_v7();
    let token = jwt
        .create_token_with_axes(Uuid::now_v7(), Uuid::now_v7(), "member", Some(workspace_id), None, Some(project_id))
        .unwrap();

    let mut parts = build_parts(&token, jwt);
    let auth = AuthUser::from_request_parts(&mut parts, &()).await.unwrap();

    assert_eq!(auth.scope.workspace_id(), Some(WorkspaceId::from(workspace_id)));
    assert_eq!(auth.scope.team_id(), None);
    assert_eq!(auth.scope.project_id(), Some(ProjectId::from(project_id)));
}
