//! Axum auth middleware — extracts and validates JWT from the `Authorization` header.
//!
//! Provides [`AuthUser`], an Axum extractor that:
//! 1. Reads `Authorization: Bearer <token>` from the request.
//! 2. Verifies the JWT signature and expiration.
//! 3. Constructs a [`TenantScope`] from the token claims.
//!
//! Handlers that need authentication simply add `AuthUser` as a parameter.

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};

use agentforge_core::{AppError, ErrorKind, OrgId, ProjectId, TeamId, TenantScope, UserId, WorkspaceId};

use crate::claims::Claims;
use crate::jwt::JwtManager;

/// Authenticated user extracted from a valid JWT token.
///
/// Use as an Axum handler parameter to require authentication:
///
/// ```ignore
/// async fn handler(auth: AuthUser) -> impl IntoResponse {
///     let scope = auth.scope; // TenantScope for DB queries
///     let role = auth.role;   // "owner", "admin", "member", "viewer"
///     // ...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// Tenant scope for database query isolation.
    pub scope: TenantScope,
    /// User's role within the organization.
    pub role: String,
    /// The raw claims from the verified JWT, in case handlers need extra fields.
    pub claims: Claims,
}

/// Axum extractor implementation.
///
/// Expects `Arc<JwtManager>` to be present in the request extensions (typically
/// added via `axum::Extension` or as part of the app state). Returns `AppError`
/// with `ErrorKind::Unauthorized` if the token is missing or invalid.
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract bearer token from Authorization header
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ErrorKind::Unauthorized)?;

        // Get JwtManager from request extensions
        let jwt = parts.extensions.get::<Arc<JwtManager>>().ok_or_else(|| {
            tracing::error!("JwtManager not found in request extensions");
            ErrorKind::Internal(anyhow::anyhow!("JwtManager not configured"))
        })?;

        // Verify token and extract claims
        let claims = jwt.verify_token(auth_header).map_err(|e| {
            tracing::debug!(error = %e, "JWT verification failed");
            ErrorKind::Unauthorized
        })?;

        // Construct TenantScope from validated claims.
        let scope = TenantScope::with_axes(
            OrgId::from(claims.org),
            UserId::from(claims.sub),
            claims.workspace_id.map(WorkspaceId::from),
            claims.team_id.map(TeamId::from),
            claims.project_id.map(ProjectId::from),
        );

        Ok(AuthUser { scope, role: claims.role.clone(), claims })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, Request};
    use uuid::Uuid;

    /// Helper: build a fake request with the given Authorization header and JwtManager extension.
    fn build_parts(auth_header: Option<&str>, jwt: Option<Arc<JwtManager>>) -> Parts {
        let mut builder = Request::builder().uri("/test");
        if let Some(header) = auth_header {
            builder = builder.header(AUTHORIZATION, HeaderValue::from_str(header).unwrap());
        }
        let mut req = builder.body(()).unwrap();
        if let Some(mgr) = jwt {
            req.extensions_mut().insert(mgr);
        }
        let (parts, _body) = req.into_parts();
        parts
    }

    const TEST_SECRET: &str = "test-secret-key-that-is-at-least-32-chars!!";

    #[tokio::test]
    async fn valid_token_extracts_auth_user() {
        let mgr = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();

        let token = mgr.create_token(user_id, org_id, "admin").unwrap();
        let bearer = format!("Bearer {token}");

        let mut parts = build_parts(Some(&bearer), Some(mgr));
        let auth = AuthUser::from_request_parts(&mut parts, &()).await.unwrap();

        assert_eq!(auth.claims.sub, user_id);
        assert_eq!(auth.claims.org, org_id);
        assert_eq!(auth.role, "admin");
        assert_eq!(auth.scope.org_id(), OrgId::from(org_id));
        assert_eq!(auth.scope.user_id(), UserId::from(user_id));
        assert_eq!(auth.scope.workspace_id(), None);
        assert_eq!(auth.scope.team_id(), None);
        assert_eq!(auth.scope.project_id(), None);
    }

    #[tokio::test]
    async fn missing_auth_header_returns_unauthorized() {
        let mgr = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let mut parts = build_parts(None, Some(mgr));
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Unauthorized));
    }

    #[tokio::test]
    async fn invalid_token_returns_unauthorized() {
        let mgr = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let mut parts = build_parts(Some("Bearer invalid.token.here"), Some(mgr));
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Unauthorized));
    }

    #[tokio::test]
    async fn non_bearer_prefix_returns_unauthorized() {
        let mgr = Arc::new(JwtManager::new(TEST_SECRET, 3600));
        let mut parts = build_parts(Some("Basic dXNlcjpwYXNz"), Some(mgr));
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Unauthorized));
    }

    #[tokio::test]
    async fn missing_jwt_manager_returns_internal_error() {
        let mut parts = build_parts(Some("Bearer some.token.here"), None);
        let result = AuthUser::from_request_parts(&mut parts, &()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err().kind, ErrorKind::Internal(_)));
    }
}
