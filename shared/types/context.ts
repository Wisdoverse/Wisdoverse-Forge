export type ContextScopeKind = 'user' | 'team' | 'project'
export type SkillScopeKind = 'org' | ContextScopeKind
export type MemoryVisibility = 'private' | 'shared'
export type ContextSensitivity = 'public' | 'internal' | 'confidential' | 'secret_detected'
export type MemoryState = 'candidate' | 'pending' | 'active' | 'needs_review' | 'revoked'
export type SkillState = 'candidate' | 'active' | 'deprecated' | 'revoked'
export type ContextCandidateKind = 'memory' | 'skill'
export type ContextCandidateState = 'pending' | 'approved' | 'rejected' | 'superseded'
export type ContextApprovalDecision = 'approved' | 'rejected'
export type ContextRefKind =
  | 'task'
  | 'run'
  | 'agent'
  | 'user'
  | 'team'
  | 'project'
  | 'source_message'
export type ContextLinkType = 'applied' | 'suggested' | 'source' | 'derived_from'
export type ContextFeedbackLabel =
  | 'useful'
  | 'stale'
  | 'wrong'
  | 'too_sensitive'
  | 'do_not_use_again'

export interface MemoryItem {
  id: string
  organization_id: string
  workspace_id: string
  owner_user_id: string
  scope_kind: ContextScopeKind
  scope_id: string
  source_task_id?: string | null
  source_run_id?: string | null
  title: string
  content_redacted: boolean
  content_encrypted: boolean
  visibility: MemoryVisibility
  sensitivity: ContextSensitivity
  provenance: Record<string, unknown>
  ttl_expires_at?: string | null
  confidence?: number | null
  last_used_at?: string | null
  last_verified_at?: string | null
  state: MemoryState
  revoked_at?: string | null
  created_at: string
  updated_at: string
}

export interface MemoryContent {
  id: string
  content: string
  content_redacted: boolean
  sensitivity: ContextSensitivity
}

export interface CreateMemoryItemRequest {
  title: string
  content: string
  redacted?: boolean
  scope_kind: ContextScopeKind
  scope_id?: string | null
  source_task_id?: string | null
  source_run_id?: string | null
  provenance?: Record<string, unknown> | null
  visibility?: MemoryVisibility | null
  ttl_expires_at?: string | null
  confidence?: number | null
}

export interface UpdateMemoryItemRequest {
  title?: string | null
  content?: string | null
  redacted?: boolean
  provenance?: Record<string, unknown> | null
  visibility?: MemoryVisibility | null
  confidence?: number | null
  last_verified_at?: string | null
}

export interface ExtendMemoryTtlRequest {
  ttl_expires_at?: string | null
}

export interface ReclassifyMemoryScopeRequest {
  scope_kind: ContextScopeKind
  scope_id?: string | null
  confirm_sensitive?: boolean
  confirm_expansion?: boolean
}

export interface ContextCandidate {
  id: string
  organization_id: string
  workspace_id: string
  source_run_id?: string | null
  target_skill_id?: string | null
  item_kind: ContextCandidateKind
  state: ContextCandidateState
  owner_user_id: string
  created_at: string
  updated_at: string
}

export interface ContextCandidateSummary {
  id: string
  workspace_id: string
  item_kind: ContextCandidateKind
  state: ContextCandidateState
  owner_user_id: string
  source_run_id?: string | null
  target_skill_id?: string | null
  proposed_scope_kind: SkillScopeKind
  source_available: boolean
  proposed_preview: Record<string, unknown>
  created_at: string
  updated_at: string
}

export interface ContextApproval {
  id: string
  candidate_id: string
  approver_user_id: string
  decision: ContextApprovalDecision
  scope_kind?: ContextScopeKind | null
  scope_id?: string | null
  ttl_at?: string | null
  sensitivity?: ContextSensitivity | null
  reason?: string | null
  self_approval: boolean
  user_attest_at?: string | null
  created_at: string
}

export interface ApproveContextCandidateRequest {
  scope_kind: ContextScopeKind
  scope_id?: string | null
  ttl_at?: string | null
  sensitivity?: ContextSensitivity | null
  reason?: string | null
  redacted?: boolean
  user_attested?: boolean
  confirm_expansion?: boolean
}

export interface RejectContextCandidateRequest {
  reason?: string | null
}

export interface ContextApprovalOutcome {
  candidate: ContextCandidate
  approval?: ContextApproval | null
  memory_item?: MemoryItem | null
  skill?: GovernedSkill | null
}

export interface ContextLink {
  id: string
  organization_id: string
  workspace_id: string
  item_id: string
  item_kind: ContextCandidateKind
  ref_id: string
  ref_kind: ContextRefKind
  link_type: ContextLinkType
  created_by_user_id: string
  created_at: string
}

export interface ContextFeedback {
  id: string
  organization_id: string
  workspace_id: string
  run_id: string
  item_id: string
  item_kind: ContextCandidateKind
  label: ContextFeedbackLabel
  note?: string | null
  user_id: string
  created_at: string
  updated_at: string
}

export interface RecordContextFeedbackRequest {
  run_id: string
  item_id: string
  item_kind: ContextCandidateKind
  label: ContextFeedbackLabel
  note?: string | null
}

export interface ContextFeedbackOutcome {
  feedback: ContextFeedback
  item_state_changed: boolean
}

export interface TaskContextRun {
  id: string
  status: string
  agentId: string
  startedAt: string
  finishedAt?: string | null
  capabilityProfile: Record<string, unknown>
}

export interface AppliedContextSource {
  sourceType: string
  sourceId?: string | null
  title?: string | null
}

export interface AppliedContextFeedback {
  label: ContextFeedbackLabel
  note?: string | null
  updatedAt: string
}

export interface AppliedContextItem {
  injectionId: string
  runId: string
  itemId: string
  itemKind: ContextCandidateKind
  position: number
  title: string
  contentPreview: string
  contentTruncated: boolean
  contentRef?: string | null
  scopeKind?: ContextScopeKind | SkillScopeKind | null
  scopeId?: string | null
  sensitivity?: ContextSensitivity | null
  state?: MemoryState | SkillState | null
  revoked: boolean
  sourceTaskId?: string | null
  sourceRunId?: string | null
  source?: AppliedContextSource | null
  lastUsedAt?: string | null
  lastVerifiedAt?: string | null
  appliedAt: string
  adapter: string
  envelopeVersion: string
  capabilityProfile: Record<string, unknown>
  degradationReason?: string | null
  feedback?: AppliedContextFeedback | null
}

export interface TaskContextCandidate {
  id: string
  itemKind: ContextCandidateKind
  state: ContextCandidateState
  ownerUserId: string
  sourceRunId?: string | null
  targetSkillId?: string | null
  proposedPreview: Record<string, unknown>
  createdAt: string
  updatedAt: string
}

export interface TaskContextEvidence {
  runId?: string | null
  sourceType: string
  sourceId: string
  agentId?: string | null
  payload: Record<string, unknown>
  createdAt: string
}

export interface TaskContextProvenance {
  runId: string
  itemId: string
  itemKind: ContextCandidateKind
  title: string
  source?: AppliedContextSource | null
  adapter: string
  envelopeVersion: string
  appliedAt: string
  state?: MemoryState | SkillState | null
  revoked: boolean
}

export interface TaskContextResponse {
  taskId: string
  runs: TaskContextRun[]
  appliedItems: AppliedContextItem[]
  suggestedMemoryUpdates: TaskContextCandidate[]
  skillCandidates: TaskContextCandidate[]
  evidence: TaskContextEvidence[]
  provenance: TaskContextProvenance[]
}

export interface ContextPreviewItem {
  id: string
  itemKind: ContextCandidateKind
  title: string
  selected: boolean
  pinned: boolean
  scopeKind?: ContextScopeKind | SkillScopeKind | null
  scopeId?: string | null
  sensitivity?: ContextSensitivity | null
  estimatedTokens: number
  lastUsedAt?: string | null
  lastVerifiedAt?: string | null
  why: string
}

export interface ContextPreviewResponse {
  contextPreviewId: string
  previewHash: string
  taskId: string
  agentId: string
  expiresAt: string
  capability: Record<string, unknown>
  degradation: string[]
  items: ContextPreviewItem[]
  suggestedItems: ContextPreviewItem[]
  previouslyPinned: ContextPreviewItem[]
  warnings: string[]
}

export interface CreateContextPreviewRequest {
  taskId: string
  agentId: string
}

export interface PublishWithContextRequest {
  contextPreviewId: string
  previewHash: string
  pinnedIds: string[]
  removedIds: string[]
}

export interface GovernedSkill {
  id: string
  organization_id?: string | null
  workspace_id?: string | null
  scope_kind?: SkillScopeKind | null
  scope_id?: string | null
  name: string
  description?: string | null
  trigger_pattern?: string | null
  negative_trigger?: string | null
  content: string
  enabled: boolean
  state: SkillState
  version: number
  owner_user_id?: string | null
  ttl_expires_at?: string | null
  sensitivity: ContextSensitivity
  provenance: Record<string, unknown>
  required_inputs: unknown[]
  tools: unknown[]
  examples: unknown[]
  success_evidence: unknown[]
  revoked_at?: string | null
  created_at: string
  updated_at: string
}

export interface SkillVersion {
  id: string
  skill_id: string
  version: number
  snapshot: GovernedSkill
  author_user_id: string
  created_at: string
}

export interface CreateGovernedSkillRequest {
  name: string
  description?: string | null
  trigger_pattern?: string | null
  negative_trigger?: string | null
  content: string
  scope_kind?: SkillScopeKind | null
  scope_id?: string | null
  state?: Exclude<SkillState, 'revoked'> | null
  sensitivity?: ContextSensitivity | null
  provenance?: Record<string, unknown> | null
  required_inputs?: unknown[] | null
  tools?: unknown[] | null
  examples?: unknown[] | null
  success_evidence?: unknown[] | null
  ttl_expires_at?: string | null
}

export interface RestoreSkillVersionRequest {
  version: number
  expected_current_version?: number | null
  confirm_expansion?: boolean
}
