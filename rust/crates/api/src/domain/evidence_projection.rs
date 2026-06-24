//! Run-scoped evidence response shape.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
    pub run_id: Option<Uuid>,
    #[serde(rename = "organizationId")]
    pub organization_id: Uuid,
    #[serde(rename = "workspaceId", skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceId")]
    pub source_id: Uuid,
    pub payload: serde_json::Value,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}
