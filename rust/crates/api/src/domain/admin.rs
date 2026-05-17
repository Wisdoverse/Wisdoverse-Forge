//! Admin domain rules.
//!
//! This module owns pure admin-console policies that are independent of
//! repositories and HTTP route DTOs.

use agentforge_core::{AgentStatus, AppResult, ErrorKind};
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

/// Columns the admin agent list can be sorted by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AdminAgentSort {
    Name,
    Status,
    #[default]
    LastActivity,
    CreatedAt,
    OwnerUsername,
}

/// Sort direction for admin list queries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AdminAgentFilterQuery<'a> {
    pub search: Option<&'a str>,
    pub status: Option<&'a str>,
    pub page: i64,
    pub limit: i64,
    pub sort_by: Option<&'a str>,
    pub sort_order: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdminAgentFilterDecision {
    pub search: Option<String>,
    pub status: Option<AgentStatus>,
    pub page: i64,
    pub limit: i64,
    pub offset: i64,
    pub sort_by: AdminAgentSort,
    pub sort_order: SortOrder,
}

/// Admin agent list parsing and pagination policy.
pub(crate) struct AdminAgentFilterPolicy;

impl AdminAgentFilterPolicy {
    pub(crate) fn from_query(query: AdminAgentFilterQuery<'_>) -> AdminAgentFilterDecision {
        let page = query.page.max(1);
        let limit = query.limit.clamp(1, 100);
        AdminAgentFilterDecision {
            search: query.search.and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }),
            status: Self::parse_status_filter(query.status),
            page,
            limit,
            offset: (page - 1) * limit,
            sort_by: Self::parse_sort_by(query.sort_by),
            sort_order: Self::parse_sort_order(query.sort_order),
        }
    }

    fn parse_status_filter(raw: Option<&str>) -> Option<AgentStatus> {
        match raw?.to_ascii_lowercase().as_str() {
            "working" => Some(AgentStatus::Working),
            "idle" => Some(AgentStatus::Idle),
            "offline" => Some(AgentStatus::Offline),
            _ => None,
        }
    }

    fn parse_sort_by(raw: Option<&str>) -> AdminAgentSort {
        match raw.unwrap_or("") {
            "name" => AdminAgentSort::Name,
            "status" => AdminAgentSort::Status,
            "createdAt" => AdminAgentSort::CreatedAt,
            "ownerUsername" => AdminAgentSort::OwnerUsername,
            _ => AdminAgentSort::LastActivity,
        }
    }

    fn parse_sort_order(raw: Option<&str>) -> SortOrder {
        match raw.unwrap_or("").to_ascii_lowercase().as_str() {
            "asc" => SortOrder::Asc,
            _ => SortOrder::Desc,
        }
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

    #[test]
    fn admin_agent_filter_accepts_supported_statuses() {
        assert_eq!(
            AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: Some("working"),
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            })
            .status,
            Some(AgentStatus::Working)
        );
        assert_eq!(
            AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: Some("WORKING"),
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            })
            .status,
            Some(AgentStatus::Working)
        );
    }

    #[test]
    fn admin_agent_filter_drops_unsupported_statuses() {
        for status in [Some("waiting"), Some("attention"), Some("bogus"), None] {
            let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status,
                page: 1,
                limit: 25,
                sort_by: None,
                sort_order: None,
            });
            assert_eq!(decision.status, None);
        }
    }

    #[test]
    fn admin_agent_filter_maps_sorting_and_defaults() {
        let cases = [
            (Some("name"), AdminAgentSort::Name),
            (Some("status"), AdminAgentSort::Status),
            (Some("createdAt"), AdminAgentSort::CreatedAt),
            (Some("ownerUsername"), AdminAgentSort::OwnerUsername),
            (Some("lastActivity"), AdminAgentSort::LastActivity),
            (Some("tokens"), AdminAgentSort::LastActivity),
            (Some("cwd"), AdminAgentSort::LastActivity),
            (None, AdminAgentSort::LastActivity),
        ];

        for (sort_by, expected) in cases {
            let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
                search: None,
                status: None,
                page: 1,
                limit: 25,
                sort_by,
                sort_order: Some("ASC"),
            });
            assert_eq!(decision.sort_by, expected);
            assert_eq!(decision.sort_order, SortOrder::Asc);
        }
    }

    #[test]
    fn admin_agent_filter_paginates_clamps_and_drops_blank_search() {
        let decision = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
            search: Some("  "),
            status: None,
            page: 4,
            limit: 10,
            sort_by: Some("name"),
            sort_order: Some("asc"),
        });

        assert_eq!(decision.search, None);
        assert_eq!(decision.page, 4);
        assert_eq!(decision.limit, 10);
        assert_eq!(decision.offset, 30);
        assert_eq!(decision.sort_by, AdminAgentSort::Name);
        assert_eq!(decision.sort_order, SortOrder::Asc);

        let clamped = AdminAgentFilterPolicy::from_query(AdminAgentFilterQuery {
            search: Some(" user@example.com "),
            status: None,
            page: -4,
            limit: 500,
            sort_by: None,
            sort_order: Some("nope"),
        });
        assert_eq!(clamped.search.as_deref(), Some("user@example.com"));
        assert_eq!(clamped.page, 1);
        assert_eq!(clamped.limit, 100);
        assert_eq!(clamped.offset, 0);
        assert_eq!(clamped.sort_order, SortOrder::Desc);
    }
}
