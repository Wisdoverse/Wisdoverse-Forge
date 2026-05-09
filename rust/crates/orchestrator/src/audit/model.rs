use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditAction {
    #[serde(rename = "task.create")]
    TaskCreate,
    #[serde(rename = "task.update")]
    TaskUpdate,
    #[serde(rename = "task.assign")]
    TaskAssign,
    #[serde(rename = "task.transition")]
    TaskTransition,
    #[serde(rename = "task.delete")]
    TaskDelete,
    #[serde(rename = "review.create")]
    ReviewCreate,
    #[serde(rename = "review.approve")]
    ReviewApprove,
    #[serde(rename = "review.reject")]
    ReviewReject,
    #[serde(rename = "review.comment")]
    ReviewComment,
    #[serde(rename = "workflow.create")]
    WorkflowCreate,
    #[serde(rename = "workflow.run")]
    WorkflowRun,
    #[serde(rename = "workflow.cancel")]
    WorkflowCancel,
    #[serde(rename = "knowledge.create")]
    KnowledgeCreate,
    #[serde(rename = "knowledge.update")]
    KnowledgeUpdate,
    #[serde(rename = "knowledge.delete")]
    KnowledgeDelete,
    #[serde(rename = "auth.login")]
    AuthLogin,
    #[serde(rename = "auth.logout")]
    AuthLogout,
    #[serde(rename = "team.create")]
    TeamCreate,
    #[serde(rename = "team.update")]
    TeamUpdate,
    #[serde(rename = "team.delete")]
    TeamDelete,
    #[serde(rename = "team.member.add")]
    TeamMemberAdd,
    #[serde(rename = "team.member.remove")]
    TeamMemberRemove,
    #[serde(rename = "rbac.grant")]
    RbacGrant,
    #[serde(rename = "rbac.revoke")]
    RbacRevoke,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TaskCreate => "task.create",
            Self::TaskUpdate => "task.update",
            Self::TaskAssign => "task.assign",
            Self::TaskTransition => "task.transition",
            Self::TaskDelete => "task.delete",
            Self::ReviewCreate => "review.create",
            Self::ReviewApprove => "review.approve",
            Self::ReviewReject => "review.reject",
            Self::ReviewComment => "review.comment",
            Self::WorkflowCreate => "workflow.create",
            Self::WorkflowRun => "workflow.run",
            Self::WorkflowCancel => "workflow.cancel",
            Self::KnowledgeCreate => "knowledge.create",
            Self::KnowledgeUpdate => "knowledge.update",
            Self::KnowledgeDelete => "knowledge.delete",
            Self::AuthLogin => "auth.login",
            Self::AuthLogout => "auth.logout",
            Self::TeamCreate => "team.create",
            Self::TeamUpdate => "team.update",
            Self::TeamDelete => "team.delete",
            Self::TeamMemberAdd => "team.member.add",
            Self::TeamMemberRemove => "team.member.remove",
            Self::RbacGrant => "rbac.grant",
            Self::RbacRevoke => "rbac.revoke",
        }
    }
}

impl FromStr for AuditAction {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "task.create" => Ok(Self::TaskCreate),
            "task.update" => Ok(Self::TaskUpdate),
            "task.assign" => Ok(Self::TaskAssign),
            "task.transition" => Ok(Self::TaskTransition),
            "task.delete" => Ok(Self::TaskDelete),
            "review.create" => Ok(Self::ReviewCreate),
            "review.approve" => Ok(Self::ReviewApprove),
            "review.reject" => Ok(Self::ReviewReject),
            "review.comment" => Ok(Self::ReviewComment),
            "workflow.create" => Ok(Self::WorkflowCreate),
            "workflow.run" => Ok(Self::WorkflowRun),
            "workflow.cancel" => Ok(Self::WorkflowCancel),
            "knowledge.create" => Ok(Self::KnowledgeCreate),
            "knowledge.update" => Ok(Self::KnowledgeUpdate),
            "knowledge.delete" => Ok(Self::KnowledgeDelete),
            "auth.login" => Ok(Self::AuthLogin),
            "auth.logout" => Ok(Self::AuthLogout),
            "team.create" => Ok(Self::TeamCreate),
            "team.update" => Ok(Self::TeamUpdate),
            "team.delete" => Ok(Self::TeamDelete),
            "team.member.add" => Ok(Self::TeamMemberAdd),
            "team.member.remove" => Ok(Self::TeamMemberRemove),
            "rbac.grant" => Ok(Self::RbacGrant),
            "rbac.revoke" => Ok(Self::RbacRevoke),
            _ => Err(format!("invalid audit action: {value}")),
        }
    }
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLog {
    pub id: String,
    pub action: AuditAction,
    pub actor_id: String,
    pub actor_type: String,
    pub resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub org_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AuditFilter {
    pub org_id: String,
    pub actor_id: Option<String>,
    pub resource: Option<String>,
    pub resource_id: Option<String>,
    pub action: Option<AuditAction>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
}
