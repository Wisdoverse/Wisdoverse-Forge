//! SCIM 2.0 projection rules: paging bounds and response/error shapes.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

pub(crate) const SCIM_USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
pub(crate) const SCIM_LIST_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
pub(crate) const SCIM_ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";

/// Paging policy: SCIM is 1-based, count clamped to 1..=100 (default 50).
pub(crate) struct ScimPagePolicy;

impl ScimPagePolicy {
    pub(crate) const MIN_COUNT: i64 = 1;
    pub(crate) const MAX_COUNT: i64 = 100;
    pub(crate) const DEFAULT_COUNT: i64 = 50;

    pub(crate) fn normalize(count: Option<i64>, start_index: Option<i64>) -> (i64, i64) {
        let count = count.unwrap_or(Self::DEFAULT_COUNT).clamp(Self::MIN_COUNT, Self::MAX_COUNT);
        let start_index = start_index.unwrap_or(1).max(1);
        (count, start_index)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScimMeta {
    pub(crate) resource_type: String,
    pub(crate) created: DateTime<Utc>,
    pub(crate) last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScimUser {
    pub(crate) schemas: Vec<String>,
    pub(crate) id: String,
    pub(crate) user_name: String,
    pub(crate) display_name: Option<String>,
    pub(crate) active: bool,
    pub(crate) meta: ScimMeta,
}

impl ScimUser {
    pub(crate) fn new(id: Uuid, user_name: String, display_name: Option<String>, created: DateTime<Utc>) -> Self {
        Self {
            schemas: vec![SCIM_USER_SCHEMA.to_string()],
            id: id.to_string(),
            user_name,
            display_name,
            active: true,
            meta: ScimMeta { resource_type: "User".to_string(), created, last_modified: created },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScimListResponse {
    pub(crate) schemas: Vec<String>,
    pub(crate) total_results: i64,
    pub(crate) start_index: i64,
    pub(crate) items_per_page: i64,
    pub(crate) resources: Vec<ScimUser>,
}

pub(crate) fn scim_error(status: &str, detail: &str) -> serde_json::Value {
    serde_json::json!({ "schemas": [SCIM_ERROR_SCHEMA], "status": status, "detail": detail })
}

pub(crate) fn scim_not_found(detail: &str) -> serde_json::Value {
    scim_error("404", detail)
}

pub(crate) fn scim_bad_request(detail: &str) -> serde_json::Value {
    scim_error("400", detail)
}

pub(crate) fn scim_unauthorized(detail: &str) -> serde_json::Value {
    scim_error("401", detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_policy_clamps_and_is_one_based() {
        assert_eq!(ScimPagePolicy::normalize(None, None), (50, 1));
        assert_eq!(ScimPagePolicy::normalize(Some(0), None), (1, 1));
        assert_eq!(ScimPagePolicy::normalize(Some(1000), Some(0)), (100, 1));
        assert_eq!(ScimPagePolicy::normalize(Some(20), Some(2)), (20, 2));
    }
}
