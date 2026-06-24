//! Runtime-neutral context envelope delivered to agent CLI adapters.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CONTEXT_ENVELOPE_VERSION_V1: &str = "v1";
pub const SUPPORTED_CONTEXT_ENVELOPE_VERSIONS: &[&str] = &[CONTEXT_ENVELOPE_VERSION_V1];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEnvelope {
    pub envelope_version: String,
    pub run_id: Uuid,
    pub task_id: Uuid,
    pub agent_id: Uuid,
    pub capability: ContextEnvelopeCapability,
    pub applied: Vec<ContextEnvelopeItem>,
    pub skills_mount: Vec<SkillMount>,
    pub degradation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEnvelopeCapability {
    pub cli_tool: String,
    pub runtime_kind: String,
    pub max_context_tokens: u32,
    pub supports_skills_mount: bool,
    pub supports_hooks: bool,
    pub supports_subagents: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextEnvelopeItemKind {
    Memory,
    Skill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEnvelopeItem {
    pub id: Uuid,
    pub kind: ContextEnvelopeItemKind,
    pub title: String,
    pub content: String,
    pub content_ref: String,
    pub sensitivity: String,
    pub source: ContextEnvelopeSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEnvelopeSource {
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMount {
    pub name: String,
    pub version: u32,
    pub path: String,
}
