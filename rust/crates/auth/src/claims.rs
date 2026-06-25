//! JWT claims structure.
//!
//! Defines the payload embedded in JWT tokens. The claims carry the user ID,
//! organization ID, role, and optional active governance axes — enough to
//! construct a [`TenantScope`] and authorize requests without a database
//! round-trip for the base tenant context.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT token payload.
///
/// Fields follow the JWT registered claim names where applicable:
/// - `sub` — subject (user ID)
/// - `exp` — expiration time (seconds since epoch)
/// - `iat` — issued at (seconds since epoch)
///
/// Custom claims:
/// - `org` — organization/tenant ID
/// - `role` — user's role within the organization
/// - `workspace_id` — active workspace execution boundary, when selected
/// - `team_id` — active team sharing axis, when selected
/// - `project_id` — active project sharing axis, when selected
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Claims {
    /// User ID (JWT "sub" claim).
    pub sub: Uuid,
    /// Organization ID.
    pub org: Uuid,
    /// Role within the organization: "owner", "admin", "member", "viewer".
    pub role: String,
    /// Active workspace execution boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    /// Active team sharing axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<Uuid>,
    /// Active project sharing axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Expiration timestamp (seconds since Unix epoch).
    pub exp: u64,
    /// Issued-at timestamp (seconds since Unix epoch).
    pub iat: u64,
}

impl Claims {
    /// Create claims without active workspace/team/project axes.
    pub fn new(sub: Uuid, org: Uuid, role: impl Into<String>, exp: u64, iat: u64) -> Self {
        Self { sub, org, role: role.into(), workspace_id: None, team_id: None, project_id: None, exp, iat }
    }

    /// Return claims with active governance axes set.
    pub fn with_scope_axes(
        mut self,
        workspace_id: Option<Uuid>,
        team_id: Option<Uuid>,
        project_id: Option<Uuid>,
    ) -> Self {
        self.workspace_id = workspace_id;
        self.team_id = team_id;
        self.project_id = project_id;
        self
    }
}

/// True when a token issued at `iat` (unix seconds) must be rejected because it
/// was issued strictly before the account's session floor
/// (`sessions_invalid_before`, unix seconds). `None` floor = never invalidated.
///
/// The comparison is `<` (not `<=`), and the floor is stored unmodified (no
/// rounding). `iat` is whole seconds (JWT standard) while the `NOW()` floor has
/// sub-second precision, so within the single wall-clock second of the reset a
/// pre-reset token and a fresh post-reset login carry the SAME truncated value —
/// they are indistinguishable at second granularity. We resolve that tie in
/// favour of availability:
///   - `<` keeps a token whose `iat` equals the floor second, so a user who logs
///     in again in the same second as their reset is NOT locked out (a `<=`
///     would permanently revoke that brand-new token — there is no later second
///     to retry into, the `iat` is fixed).
///   - The cost is that a token minted in the *exact same second* as the reset
///     survives. That is a <=1s residual requiring an attacker to have
///     authenticated in the very second of the victim's reset; the real threat —
///     a copied refresh token inside its multi-day lifetime — has an `iat` from a
///     prior second and is always revoked.
///
/// ponytail: a monotonic per-user token-version counter embedded in the claims
/// would remove the second-granularity tie entirely, but that is a larger
/// Claims-schema + mint-path change unwarranted for a <=1s unexploitable window.
pub fn session_token_revoked(iat: u64, floor_secs: Option<i64>) -> bool {
    match floor_secs {
        Some(floor) => (iat as i64) < floor,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_roundtrip() {
        let claims = Claims::new(Uuid::now_v7(), Uuid::now_v7(), "admin", 1_700_000_000, 1_699_999_000)
            .with_scope_axes(Some(Uuid::now_v7()), Some(Uuid::now_v7()), Some(Uuid::now_v7()));

        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(claims, deserialized);
    }

    #[test]
    fn deserialize_from_json_object() {
        let user_id = Uuid::now_v7();
        let org_id = Uuid::now_v7();
        let json = serde_json::json!({
            "sub": user_id,
            "org": org_id,
            "role": "member",
            "exp": 1_700_000_000u64,
            "iat": 1_699_999_000u64,
        });

        let claims: Claims = serde_json::from_value(json).unwrap();
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.org, org_id);
        assert_eq!(claims.role, "member");
        assert_eq!(claims.workspace_id, None);
        assert_eq!(claims.team_id, None);
        assert_eq!(claims.project_id, None);
    }

    #[test]
    fn deserialize_scope_axes_from_json_object() {
        let workspace_id = Uuid::now_v7();
        let team_id = Uuid::now_v7();
        let project_id = Uuid::now_v7();
        let json = serde_json::json!({
            "sub": Uuid::now_v7(),
            "org": Uuid::now_v7(),
            "role": "member",
            "workspace_id": workspace_id,
            "team_id": team_id,
            "project_id": project_id,
            "exp": 1_700_000_000u64,
            "iat": 1_699_999_000u64,
        });

        let claims: Claims = serde_json::from_value(json).unwrap();
        assert_eq!(claims.workspace_id, Some(workspace_id));
        assert_eq!(claims.team_id, Some(team_id));
        assert_eq!(claims.project_id, Some(project_id));
    }

    #[test]
    fn session_token_revoked_when_no_floor_is_never_revoked() {
        assert!(!session_token_revoked(0, None));
        assert!(!session_token_revoked(u64::MAX, None));
    }

    #[test]
    fn session_token_revoked_rejects_tokens_issued_before_floor() {
        // Strictly before the floor second -> revoked (the real threat: a copied
        // refresh token from a prior second).
        assert!(session_token_revoked(1_699_999_000, Some(1_700_000_000)));
        assert!(session_token_revoked(1_699_999_999, Some(1_700_000_000)));
    }

    #[test]
    fn session_token_revoked_keeps_same_second_and_later_tokens() {
        // Same truncated second as the floor -> kept, so a legitimate immediate
        // post-reset login is never permanently locked out (its `iat` is fixed
        // and equals the floor second).
        assert!(!session_token_revoked(1_700_000_000, Some(1_700_000_000)));
        // Strictly after -> kept.
        assert!(!session_token_revoked(1_700_000_500, Some(1_700_000_000)));
    }
}
