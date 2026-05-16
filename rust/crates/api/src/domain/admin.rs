//! Admin domain rules.
//!
//! This module owns pure admin-console policies that are independent of
//! repositories and HTTP route DTOs.

use agentforge_core::{AppResult, ErrorKind};
use uuid::Uuid;

/// Validated pagination request for admin list endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdminListPage {
    limit: i64,
    offset: i64,
}

impl AdminListPage {
    pub(crate) fn new(limit: i64, offset: i64) -> Self {
        Self { limit: limit.clamp(1, 100), offset: offset.max(0) }
    }

    pub(crate) fn limit(self) -> i64 {
        self.limit
    }

    pub(crate) fn offset(self) -> i64 {
        self.offset
    }
}

/// Admin role authorization policy.
pub(crate) struct AdminRolePolicy;

impl AdminRolePolicy {
    pub(crate) fn require_admin(auth_role: &str) -> AppResult<()> {
        match auth_role {
            "owner" | "admin" => Ok(()),
            _ => Err(ErrorKind::Forbidden.into()),
        }
    }
}

/// Admin impersonation policy.
pub(crate) struct AdminImpersonationPolicy;

impl AdminImpersonationPolicy {
    pub(crate) fn ensure_not_self(current_user_id: Uuid, target_user_id: Uuid) -> AppResult<()> {
        if current_user_id == target_user_id {
            return Err(ErrorKind::Validation("cannot impersonate yourself".into()).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_list_page_clamps_limit_and_offset() {
        assert_eq!(AdminListPage::new(0, -10).limit(), 1);
        assert_eq!(AdminListPage::new(200, 10).limit(), 100);
        assert_eq!(AdminListPage::new(50, -10).offset(), 0);
        assert_eq!(AdminListPage::new(50, 10).offset(), 10);
    }

    #[test]
    fn admin_role_policy_accepts_owner_and_admin() {
        assert!(AdminRolePolicy::require_admin("owner").is_ok());
        assert!(AdminRolePolicy::require_admin("admin").is_ok());
    }

    #[test]
    fn admin_role_policy_rejects_non_admin_roles() {
        assert!(AdminRolePolicy::require_admin("member").is_err());
        assert!(AdminRolePolicy::require_admin("viewer").is_err());
        assert!(AdminRolePolicy::require_admin("").is_err());
    }

    #[test]
    fn admin_impersonation_policy_rejects_self_target() {
        let user_id = Uuid::now_v7();
        assert!(AdminImpersonationPolicy::ensure_not_self(user_id, user_id).is_err());
        assert!(AdminImpersonationPolicy::ensure_not_self(user_id, Uuid::now_v7()).is_ok());
    }
}
