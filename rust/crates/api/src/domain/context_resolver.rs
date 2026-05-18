//! Domain models and pure selection policies for context resolution.

use std::collections::{HashMap, HashSet};

use agentforge_core::{AgentId, RuntimeCapability, ScopedRead};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemKind {
    Memory,
    Skill,
}

impl ContextItemKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Skill => "skill",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationReason {
    BudgetTruncated,
    RuntimeCapabilityFallback,
}

impl DegradationReason {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::BudgetTruncated => "budget_truncated",
            Self::RuntimeCapabilityFallback => "runtime_capability_fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedItemRef {
    pub id: Uuid,
    pub kind: ContextItemKind,
    pub title: String,
    pub scope_kind: Option<String>,
    pub scope_id: Option<Uuid>,
    pub sensitivity: Option<String>,
    pub estimated_tokens: u32,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedContext {
    pub applied: Vec<ResolvedItemRef>,
    pub suggested: Vec<ResolvedItemRef>,
    pub capability: RuntimeCapability,
    pub degradation: Vec<DegradationReason>,
    pub envelope_version: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContextSelection {
    pub pinned_item_ids: Vec<Uuid>,
    pub removed_item_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct SelectedContext {
    pub resolved: ResolvedContext,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ContextTaskSnapshot {
    pub task_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub params: Option<Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryCandidate {
    pub(crate) id: Uuid,
    pub(crate) title: String,
    pub(crate) scope_kind: String,
    pub(crate) scope_id: Uuid,
    pub(crate) sensitivity: String,
    pub(crate) estimated_tokens: i64,
    pub(crate) last_used_at: Option<DateTime<Utc>>,
    pub(crate) last_verified_at: Option<DateTime<Utc>>,
    pub(crate) confidence: Option<f64>,
}

pub fn apply_context_selection(resolved: ResolvedContext, selection: &ContextSelection) -> SelectedContext {
    let ResolvedContext {
        applied: default_applied,
        suggested: default_suggested,
        capability,
        degradation,
        envelope_version,
    } = resolved;
    let mut warnings = Vec::new();
    let removed: HashSet<Uuid> = selection.removed_item_ids.iter().copied().collect();
    let pinned: HashSet<Uuid> = selection.pinned_item_ids.iter().copied().collect();
    let mut all_items: Vec<ResolvedItemRef> = default_applied.iter().chain(default_suggested.iter()).cloned().collect();
    all_items.sort_by_key(|item| (item.kind as u8, item.id));

    let mut by_id = HashMap::new();
    for item in all_items {
        by_id.insert(item.id, item);
    }

    let mut applied = Vec::new();
    for id in &selection.pinned_item_ids {
        if removed.contains(id) {
            continue;
        }
        if let Some(item) = by_id.get(id) {
            if item.kind != ContextItemKind::Memory {
                warnings.push(format!("pinned item {id} is not injectable by the current envelope"));
                continue;
            }
            if default_suggested.iter().any(|candidate| candidate.id == *id) {
                warnings.push(format!("pinned item {id} was outside the default selection"));
            }
            applied.push(item.clone());
        }
    }

    for item in &default_applied {
        if removed.contains(&item.id) || pinned.contains(&item.id) {
            continue;
        }
        applied.push(item.clone());
    }

    let applied_ids: HashSet<Uuid> = applied.iter().map(|item| item.id).collect();
    let suggested = default_suggested
        .into_iter()
        .chain(default_applied)
        .filter(|item| !applied_ids.contains(&item.id) && !removed.contains(&item.id))
        .collect();

    SelectedContext {
        resolved: ResolvedContext { applied, suggested, capability, degradation, envelope_version },
        warnings,
    }
}

pub(crate) fn apply_budget(
    rows: Vec<MemoryCandidate>,
    max_context_tokens: u32,
) -> (Vec<ResolvedItemRef>, Vec<ResolvedItemRef>, bool) {
    let mut used = 0_u32;
    let mut truncated = false;
    let mut applied = Vec::new();
    let mut suggested = Vec::new();

    for row in rows {
        let estimated_tokens = u32::try_from(row.estimated_tokens.max(1)).unwrap_or(u32::MAX);
        let item = ResolvedItemRef {
            id: row.id,
            kind: ContextItemKind::Memory,
            title: row.title,
            scope_kind: Some(row.scope_kind),
            scope_id: Some(row.scope_id),
            sensitivity: Some(row.sensitivity),
            estimated_tokens,
            last_used_at: row.last_used_at,
            last_verified_at: row.last_verified_at,
            why: memory_why(row.confidence, row.last_verified_at),
        };
        if used.saturating_add(estimated_tokens) <= max_context_tokens {
            used = used.saturating_add(estimated_tokens);
            applied.push(item);
        } else {
            truncated = true;
            suggested.push(item);
        }
    }

    (applied, suggested, truncated)
}

pub(crate) fn task_search_text(snapshot: &ContextTaskSnapshot) -> String {
    let mut parts = vec![snapshot.title.clone()];
    if let Some(description) = &snapshot.description {
        parts.push(description.clone());
    }
    if let Some(params) = &snapshot.params {
        for key in ["task", "message"] {
            if let Some(value) = params.get(key).and_then(|value| value.as_str()) {
                parts.push(value.to_string());
            }
        }
    }
    parts.join(" ")
}

pub(crate) fn push_degradation(items: &mut Vec<DegradationReason>, reason: DegradationReason) {
    if !items.contains(&reason) {
        items.push(reason);
    }
}

pub(crate) fn scope_hash(proof: &ScopedRead) -> String {
    let mut workspaces: Vec<String> = proof.workspace_ids().iter().map(|id| id.as_uuid().to_string()).collect();
    let mut teams: Vec<String> = proof.team_ids().iter().map(|id| id.as_uuid().to_string()).collect();
    let mut projects: Vec<String> = proof.project_ids().iter().map(|id| id.as_uuid().to_string()).collect();
    workspaces.sort();
    teams.sort();
    projects.sort();
    let material = serde_json::json!({
        "org_id": proof.org_id().as_uuid(),
        "user_id": proof.user_id().as_uuid(),
        "workspace_ids": workspaces,
        "team_ids": teams,
        "project_ids": projects,
    });
    hex::encode(Sha256::digest(material.to_string().as_bytes()))
}

pub(crate) fn context_resolver_cache_key(task_id: Uuid, agent_id: AgentId, proof: &ScopedRead) -> String {
    format!("context_resolver:{task_id}:{}:{}", agent_id.as_uuid(), scope_hash(proof))
}

fn memory_why(confidence: Option<f64>, last_verified_at: Option<DateTime<Utc>>) -> String {
    match (confidence, last_verified_at) {
        (Some(confidence), Some(last_verified_at)) => {
            format!("Matched task text; confidence {:.2}; verified {}.", confidence, last_verified_at.to_rfc3339())
        }
        (Some(confidence), None) => format!("Matched task text; confidence {:.2}.", confidence),
        (None, Some(last_verified_at)) => format!("Matched task text; verified {}.", last_verified_at.to_rfc3339()),
        (None, None) => "Matched task text.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use agentforge_core::{OrgId, ProjectId, TeamId, UserId, WorkspaceId};

    use super::*;

    fn capability() -> RuntimeCapability {
        RuntimeCapability::api_provider_or_default("provider", 10)
    }

    fn item(id: Uuid, kind: ContextItemKind, estimated_tokens: u32) -> ResolvedItemRef {
        ResolvedItemRef {
            id,
            kind,
            title: format!("item-{id}"),
            scope_kind: None,
            scope_id: None,
            sensitivity: None,
            estimated_tokens,
            last_used_at: None,
            last_verified_at: None,
            why: "Matched task text.".to_string(),
        }
    }

    #[test]
    fn task_search_text_includes_task_and_message_params_only() {
        let snapshot = ContextTaskSnapshot {
            task_id: Uuid::nil(),
            title: "title".to_string(),
            description: Some("description".to_string()),
            params: Some(serde_json::json!({
                "task": "task body",
                "message": "message body",
                "ignored": "not indexed",
            })),
        };

        assert_eq!(task_search_text(&snapshot), "title description task body message body");
    }

    #[test]
    fn apply_budget_splits_memory_candidates_by_runtime_tokens() {
        let first = Uuid::now_v7();
        let second = Uuid::now_v7();
        let (applied, suggested, truncated) = apply_budget(
            vec![
                MemoryCandidate {
                    id: first,
                    title: "first".to_string(),
                    scope_kind: "project".to_string(),
                    scope_id: Uuid::now_v7(),
                    sensitivity: "internal".to_string(),
                    estimated_tokens: 4,
                    last_used_at: None,
                    last_verified_at: None,
                    confidence: Some(0.8),
                },
                MemoryCandidate {
                    id: second,
                    title: "second".to_string(),
                    scope_kind: "project".to_string(),
                    scope_id: Uuid::now_v7(),
                    sensitivity: "internal".to_string(),
                    estimated_tokens: 7,
                    last_used_at: None,
                    last_verified_at: None,
                    confidence: None,
                },
            ],
            10,
        );

        assert!(truncated);
        assert_eq!(applied.iter().map(|item| item.id).collect::<Vec<_>>(), vec![first]);
        assert_eq!(suggested.iter().map(|item| item.id).collect::<Vec<_>>(), vec![second]);
        assert!(applied[0].why.contains("confidence 0.80"));
    }

    #[test]
    fn apply_context_selection_preserves_manual_memory_pins_and_warnings() {
        let applied_id = Uuid::now_v7();
        let suggested_memory_id = Uuid::now_v7();
        let suggested_skill_id = Uuid::now_v7();
        let resolved = ResolvedContext {
            applied: vec![item(applied_id, ContextItemKind::Memory, 1)],
            suggested: vec![
                item(suggested_memory_id, ContextItemKind::Memory, 1),
                item(suggested_skill_id, ContextItemKind::Skill, 0),
            ],
            capability: capability(),
            degradation: vec![DegradationReason::BudgetTruncated],
            envelope_version: "v1".to_string(),
        };

        let selected = apply_context_selection(
            resolved,
            &ContextSelection {
                pinned_item_ids: vec![suggested_memory_id, suggested_skill_id],
                removed_item_ids: vec![applied_id],
            },
        );

        assert_eq!(selected.resolved.applied.iter().map(|item| item.id).collect::<Vec<_>>(), vec![suggested_memory_id]);
        assert_eq!(
            selected.resolved.suggested.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![suggested_skill_id]
        );
        assert!(selected.warnings.iter().any(|warning| warning.contains("outside the default selection")));
        assert!(selected.warnings.iter().any(|warning| warning.contains("not injectable")));
    }

    #[test]
    fn degradation_reason_labels_are_protocol_stable() {
        assert_eq!(DegradationReason::BudgetTruncated.label(), "budget_truncated");
        assert_eq!(DegradationReason::RuntimeCapabilityFallback.label(), "runtime_capability_fallback");
    }

    #[test]
    fn cache_key_owns_task_agent_and_scope_contract() {
        let task_id = Uuid::from_u128(0x11111111111141118111111111111111);
        let agent_id = AgentId::from(Uuid::from_u128(0x22222222222242228222222222222222));
        let org_id = OrgId::from(Uuid::from_u128(0x33333333333343338333333333333333));
        let user_id = UserId::from(Uuid::from_u128(0x44444444444444448444444444444444));
        let workspace_id = WorkspaceId::from(Uuid::from_u128(0x55555555555545558555555555555555));
        let read = ScopedRead::from_validated_memberships(
            org_id,
            user_id,
            [workspace_id],
            [TeamId::from(Uuid::from_u128(0x66666666666646668666666666666666))],
            [ProjectId::from(Uuid::from_u128(0x77777777777747778777777777777777))],
        );
        let other_read = ScopedRead::from_validated_memberships(org_id, user_id, [workspace_id], [], []);

        let key = context_resolver_cache_key(task_id, agent_id, &read);

        assert!(key.starts_with(&format!("context_resolver:{task_id}:{}:", agent_id.as_uuid())));
        assert_eq!(key, context_resolver_cache_key(task_id, agent_id, &read));
        assert_ne!(key, context_resolver_cache_key(task_id, agent_id, &other_read));
    }
}
