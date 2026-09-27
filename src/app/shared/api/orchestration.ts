import type {
  ApproveContextCandidateRequest,
  ContextApprovalOutcome,
  ContextCandidateKind,
  ContextCandidateState,
  ContextCandidateSummary,
  ContextFeedbackOutcome,
  ContextPreviewResponse,
  ContextScopeKind,
  MemoryContent,
  PublishWithContextRequest,
  RecordContextFeedbackRequest,
  RejectContextCandidateRequest,
  TaskContextResponse,
} from '@shared/types/context'
import { authFetch } from './authFetch'

export interface RecurringTask {
  id: string
  name: string
  title: string
  description: string
  priority: string
  requiresApproval: boolean
  projectId: string
  groupId: string
  cadenceMinutes: number
  nextRunAt: string
  enabled: boolean
  createdAt: string
}

export interface CreateRecurringTaskInput {
  name: string
  title: string
  description?: string
  priority?: string
  requiresApproval?: boolean
  projectId: string
  groupId: string
  cadenceMinutes: number
}

export interface ReviewGateStatus {
  requiredKeys: string[]
  satisfied: boolean
  missing: string[]
}

export type TaskState =
  'backlog' | 'queued' | 'working' | 'blocked' | 'completed' | 'failed' | 'canceled'

export type BlockedReason =
  | 'waiting_agent'
  | 'waiting_dependency'
  | 'waiting_input'
  | 'waiting_approval'
  | 'quota_exceeded'
  | 'waiting_verification'

export interface TaskResultArtifact {
  name: string
  mimeType: string
  data: string
}

export type TaskResultPayload =
  | TaskResultArtifact[]
  | {
      stdout?: string
      message?: string
      [key: string]: unknown
    }

export interface TaskContextCounts {
  appliedMemories: number
  appliedSkills: number
  total: number
}

export interface TaskRunSummary {
  id: string
  agentId: string
  status: string
  startedAt: string
  finishedAt?: string
  runtimeKind?: string
  cliTool?: string
  providerName?: string
  maxContextTokens?: number
}

export type TaskCommentKind = 'comment' | 'blocker' | 'unblock'

export interface TaskComment {
  id: string
  taskId: string
  kind: TaskCommentKind
  body: string
  author: {
    id: string
    name: string
  }
  createdAt: string
  updatedAt: string
}

/** Latest human blocker/unblock signal for a task, used for board badges. */
export interface HumanMark {
  taskId: string
  kind: TaskCommentKind
  body: string
  authorName?: string
  createdAt: string
}

/** One skill an agent follows (attach-back from the agent side). */
export interface FollowedSkill {
  skillId: string
  name: string
  state: string
}

/** One ticked or unticked human review check on a task. */
export interface TaskReviewCheck {
  checkKey: string
  done: boolean
  updatedAt: string
}

export interface TaskSummary {
  id: string
  groupId?: string
  state: TaskState
  method: string
  params: { task: string; message: string }
  assignedTo?: string
  assignedAgentName?: string
  /** Human owner for lifecycle notifications. Currently the task creator. */
  createdBy?: string
  priority: 'low' | 'normal' | 'high' | 'urgent'
  progress: number
  error?: string
  result?: TaskResultPayload
  /** Reason a `blocked` task is stuck. Drives the kanban "还差什么" hint. */
  blockedReason?: BlockedReason
  /** Human-readable hint already localized server-side (e.g. "等待空闲 agent (2 个忙碌)"). */
  blockedHint?: string
  /** Reason-specific structured payload (counts, missing fields, approver, etc.). */
  blockedMetadata?: Record<string, unknown>
  createdAt: string
  updatedAt: string
  /** Database-owned monotonic revision. Absent only during rolling upgrades. */
  rowVersion?: number
  completedAt?: string
  contextCounts?: TaskContextCounts
  /** 1-based attempt counter; incremented on each retry. */
  attempt: number
  /** RFC3339 timestamp when the current worker lease expires (only set while working). */
  leaseExpiresAt?: string
  /** True when this is a self-fix task (a code-fix against this repo). Drives the Review tab. */
  selfFix?: boolean
  /** Draft-PR number once the self-fix Bridge has opened one. */
  prNumber?: number
  /** Canonical PR URL. */
  prUrl?: string
  /** PR head SHA recorded at open time (the merge gate re-verifies against it). */
  prHeadSha?: string
  /** Persisted self-fix review status (mirrors the Rust `review_status` vocabulary). */
  reviewStatus?: SelfFixReviewStatus
  /** Queued-time dispatch prediction (present only while waiting). */
  waitEstimate?: TaskWaitEstimate
}

/** Server-computed prediction for when a waiting task's agent will start it. */
export interface TaskWaitEstimate {
  /** 1-based position in the effective queue (same agent, else shared pool). */
  position: number
  /** Median duration of recently completed tasks in this org, seconds. 0 = no history yet. */
  typicalSeconds: number
  /** Predicted seconds until the agent starts this task. */
  estimatedSeconds: number
}

/** Self-fix review-status vocabulary — mirrors `domain::self_fix::review_status` (Rust). */
export type SelfFixReviewStatus =
  'in_review' | 'approved' | 'changes_requested' | 'merged' | 'sensitive_blocked'

/**
 * Read-side review snapshot for a self-fix task's draft PR. Mirrors the Rust
 * `SelfFixReview` serializer (camelCase) field-for-field. Approve is enabled in
 * the UI only when `checksGreen && !sensitive` — both are computed server-side.
 */
export interface SelfFixReview {
  taskId: string
  prNumber?: number
  prUrl?: string
  diffUrl?: string
  headSha?: string
  checksGreen: boolean
  sensitive: boolean
  reviewStatus?: SelfFixReviewStatus
}

export function taskResultArtifacts(result: TaskSummary['result']): TaskResultArtifact[] {
  if (!result) return []

  if (Array.isArray(result)) {
    return result.filter(
      (item): item is TaskResultArtifact =>
        Boolean(item) &&
        typeof item.name === 'string' &&
        typeof item.mimeType === 'string' &&
        typeof item.data === 'string'
    )
  }

  if (typeof result.stdout === 'string') {
    return [{ name: 'text-result.txt', mimeType: 'text/plain', data: result.stdout }]
  }

  if (typeof result.message === 'string') {
    return [{ name: 'final-answer.txt', mimeType: 'text/plain', data: result.message }]
  }

  return [
    {
      name: 'result.json',
      mimeType: 'application/json',
      data: JSON.stringify(result, null, 2),
    },
  ]
}

export interface ParticipantSummary {
  id: string
  agentId: string
  name: string
  status: 'available' | 'busy' | 'offline'
  capabilities: string[]
  /** Agent runtime kind ('container' | 'cli' | 'api'), used to gate
   * runtime-specific affordances such as task image upload. Omitted by older
   * servers / when the agent row could not be resolved. */
  runtimeKind?: 'container' | 'cli' | 'api'
  lastHeartbeatAt?: string
}

export interface TaskStats {
  byState: Record<string, number>
  queueStats?: {
    waiting: number
    active: number
    completed: number
    failed: number
    delayed: number
  }
}

export interface ListContextCandidatesParams {
  state?: ContextCandidateState | 'all'
  itemKind?: ContextCandidateKind | 'all'
  scopeKind?: ContextScopeKind | 'all'
  limit?: number
  offset?: number
}

export interface ContextUsageItem {
  itemId: string
  itemKind: 'memory' | 'skill'
  itemTitle: string
  scopeKind?: string | null
  scopeId?: string | null
  itemState?: string | null
  sensitivity?: string | null
  lastVerifiedAt?: string | null
  taskKind: string
  runtime: string
  agentId: string
  agentName: string
  appliedCount: number
  completedCount: number
  successRate: number
  feedbackTotalCount: number
  feedbackUsefulCount: number
  feedbackNegativeCount: number
  negativeFeedbackRate: number
  lastUsedAt: string
  lastFeedbackAt?: string | null
}

export interface ContextUsageAnalytics {
  lastRefreshedAt: string
  lastRefreshStartedAt?: string | null
  lastRefreshError?: string | null
  staleAfterHours: number
  isStale: boolean
  query: {
    limit: number
    minApplied: number
    staleAfterDays: number
    minSuccessRate: number
    negativeRate: number
  }
  summary: {
    rowCount: number
    distinctItems: number
    distinctAgents: number
    appliedCount: number
    completedCount: number
    successRate: number
    feedbackUsefulCount: number
    feedbackNegativeCount: number
  }
  topUseful: ContextUsageItem[]
  staleItems: ContextUsageItem[]
  needsReview: ContextUsageItem[]
}

export interface ContextFeatureSnapshot {
  governance: boolean
  preview: boolean
  injection: boolean
  analytics: boolean
}

export interface TaskTemplate {
  id: string
  name: string
  title: string
  description: string
  priority: string
  requiresApproval: boolean
  projectId?: string | null
  createdBy: string
  createdAt: string
}

export interface CreateTaskTemplateInput {
  name: string
  title: string
  description?: string
  priority?: string
  requiresApproval?: boolean
  projectId?: string
}

export interface AgentReliabilityEntry {
  agentId: string
  name: string | null
  total: number
  succeeded: number
  failed: number
  successRate: number
}

export interface AgentReliabilityReport {
  windowHours: number
  agents: AgentReliabilityEntry[]
}

export interface AgentUsageEntry {
  agentId: string
  name: string | null
  requests: number
  tokensIn: number
  tokensOut: number
  totalTokens: number
  share: number
  estimatedCost?: number | null
}

export interface AgentUsageReport {
  windowHours: number
  pricingConfigured: boolean
  agents: AgentUsageEntry[]
}

export type GovernanceAuditItemKind = 'memory' | 'skill'
export type GovernanceAuditScopeKind = 'org' | 'user' | 'workspace' | 'team' | 'project'
export type GovernanceAuditTamperStatus = 'not_configured' | 'valid' | 'invalid'

export interface GovernanceAuditQueryParams {
  eventType?: string
  eventPrefix?: string
  itemKind?: GovernanceAuditItemKind
  scopeKind?: GovernanceAuditScopeKind
  scopeId?: string
  userId?: string
  from?: string
  to?: string
  redactSecrets?: boolean
  limit?: number
  offset?: number
}

export interface GovernanceAuditEntry {
  id: string
  eventType: string
  actorUserId?: string | null
  itemKind?: GovernanceAuditItemKind | null
  scopeKind?: GovernanceAuditScopeKind | null
  scopeId?: string | null
  rawItemId?: string | null
  auditSubjectHash: string
  resourceType: string
  resourceId?: string | null
  details: unknown
  detailsRedacted: boolean
  tamperStatus: GovernanceAuditTamperStatus
  createdAt: string
}

export interface GovernanceAuditResponse {
  entries: GovernanceAuditEntry[]
  query: {
    eventPrefix: string
    limit: number
    offset: number
    redacted: boolean
  }
}

export interface InboxNotificationDto {
  id: string
  type: 'blocked' | 'completed' | 'failed' | 'assigned' | 'mentioned' | 'credential_expired'
  taskId: string
  taskTitle: string
  message: string
  taskHref?: string
  ownerUserId?: string
  read: boolean
  timestamp: number
}

const API_V1_BASE = '/api/v1'
const API_BASE = `${API_V1_BASE}/orchestration`

async function apiFetch<T>(path: string, options?: RequestInit): Promise<T> {
  return apiFetchFrom<T>(API_BASE, path, options)
}

async function apiV1Fetch<T>(path: string, options?: RequestInit): Promise<T> {
  return apiFetchFrom<T>(API_V1_BASE, path, options)
}

async function apiFetchFrom<T>(base: string, path: string, options?: RequestInit): Promise<T> {
  // F068/F075: route the task-board clients through the shared auth-aware fetch
  // (token injection + 401 refresh/retry) so an access token that expires
  // between scheduled refreshes recovers transparently instead of 401-ing every
  // getTasks/createTask/updateTask/approveSelfFix call with a raw `API 401`.
  const res = await authFetch(`${base}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...(options?.headers instanceof Headers
        ? Object.fromEntries(options.headers.entries())
        : (options?.headers ?? {})),
    },
  })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(`API ${res.status}: ${body}`)
  }
  return res.json() as Promise<T>
}

export const orchestrationApi = {
  getTasks: async (
    groupId: string,
    params?: { state?: string; groupBy?: string }
  ): Promise<TaskSummary[]> => {
    const res = await apiFetch<{ ok: boolean; tasks: TaskSummary[] }>(
      `/groups/${groupId}/tasks${params ? `?${new URLSearchParams(Object.fromEntries(Object.entries(params).filter(([, v]) => v !== undefined)))}` : ''}`
    )
    return res.tasks
  },
  getStats: async (groupId: string): Promise<TaskStats> => {
    const res = await apiFetch<{ ok: boolean; stats: TaskStats }>(`/groups/${groupId}/tasks/stats`)
    return res.stats
  },
  /**
   * Tasks assigned to a specific agent — backs the Tasks tab on the agent detail page.
   * Server enforces tenant scope so we just pass the agent UUID.
   */
  getTasksByAgent: async (
    agentId: string,
    params?: { status?: string; limit?: number }
  ): Promise<TaskSummary[]> => {
    const search = new URLSearchParams({ agentId })
    if (params?.status) search.set('status', params.status)
    if (params?.limit) search.set('limit', String(params.limit))
    const res = await apiFetch<{ ok: boolean; tasks: TaskSummary[] }>(`/tasks?${search.toString()}`)
    return res.tasks
  },
  createTask: (data: {
    groupId: string
    params: {
      task: string
      message: string
      requiredInputs?: string[]
      inputs?: Record<string, unknown>
      env?: Record<string, unknown>
      apiKeys?: Record<string, unknown>
      imageAttachmentIds?: string[]
      dependencyIds?: string[]
    }
    priority?: string
    assignedTo?: string
    requiresApproval?: boolean
  }) =>
    apiFetch<{ ok: boolean; task: TaskSummary }>('/tasks', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  updateTask: (
    taskId: string,
    data: { state?: string; priority?: string; progress?: number; assignedTo?: string }
  ) =>
    apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    }),
  assignTask: (taskId: string, assignedTo: string) =>
    apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}`, {
      method: 'PATCH',
      body: JSON.stringify({ assignedTo }),
    }),
  cancelTask: (taskId: string) =>
    apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}/cancel`, { method: 'POST' }),
  retryTask: (taskId: string) =>
    apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}/retry`, { method: 'POST' }),
  approveTask: (taskId: string) =>
    apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}/approve`, { method: 'POST' }),
  /** Fetch the self-fix draft-PR review snapshot (diff link, live CI verdict, sensitive flag). */
  getSelfFixReview: async (taskId: string): Promise<SelfFixReview> => {
    const res = await apiV1Fetch<{ ok: boolean; data: SelfFixReview }>(
      `/self-fix/tasks/${taskId}/review`
    )
    return res.data
  },
  /** Operator-approve a self-fix PR → server-side guarded merge. Returns the new review status. */
  approveSelfFix: async (taskId: string): Promise<SelfFixReviewStatus> => {
    await apiV1Fetch<{ ok: boolean; data: { prNumber: number; alreadyMerged: boolean } }>(
      `/self-fix/tasks/${taskId}/approve`,
      { method: 'POST' }
    )
    return 'merged'
  },
  fetchContextForTask: async (taskId: string): Promise<TaskContextResponse> => {
    const res = await apiFetch<{ ok: boolean; data: TaskContextResponse }>(
      `/tasks/${taskId}/context`
    )
    return res.data
  },
  getTask: async (taskId: string): Promise<TaskSummary> => {
    const res = await apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}`)
    return res.task
  },

  getTaskRuns: async (taskId: string): Promise<TaskRunSummary[]> => {
    const res = await apiFetch<{ ok: boolean; runs: TaskRunSummary[] }>(`/tasks/${taskId}/runs`)
    return res.runs
  },
  getTaskComments: async (taskId: string): Promise<TaskComment[]> => {
    const res = await apiFetch<{ ok: boolean; comments: TaskComment[] }>(
      `/tasks/${taskId}/comments`
    )
    return res.comments
  },
  createTaskComment: async (
    taskId: string,
    input: { kind?: TaskCommentKind; body: string }
  ): Promise<TaskComment> => {
    const res = await apiFetch<{ ok: boolean; comment: TaskComment }>(`/tasks/${taskId}/comments`, {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return res.comment
  },
  deleteTaskComment: async (taskId: string, commentId: string): Promise<void> => {
    await apiFetch<{ ok: boolean }>(`/tasks/${taskId}/comments/${commentId}`, {
      method: 'DELETE',
    })
  },
  getLatestHumanMarks: async (taskIds: string[]): Promise<HumanMark[]> => {
    if (taskIds.length === 0) return []
    const res = await apiFetch<{ ok: boolean; marks: HumanMark[] }>(
      `/tasks/comments/latest?taskIds=${encodeURIComponent(taskIds.join(','))}`
    )
    return res.marks
  },
  exportTaskHistoryCsv: async (limit?: number): Promise<string> => {
    const query = limit ? `?limit=${limit}` : ''
    const res = await apiFetch<{ ok: boolean; format: string; content: string }>(
      `/tasks/export${query}`
    )
    return res.content
  },
  getAgentFollowedSkills: async (agentId: string): Promise<FollowedSkill[]> => {
    const res = await apiFetch<{ ok: boolean; skills: FollowedSkill[] }>(
      `/agents/${encodeURIComponent(agentId)}/skills`
    )
    return res.skills
  },
  /** Best-effort product analytics event (skill draft acceptance measurement). */
  trackProductEvent: async (
    eventName: string,
    properties: Record<string, unknown> = {}
  ): Promise<void> => {
    try {
      await apiV1Fetch('/analytics/events', {
        method: 'POST',
        body: JSON.stringify({ event_name: eventName, properties }),
      })
    } catch {
      // Analytics is best-effort; a blocked event never breaks the draft flow.
    }
  },
  listAnalyticsEvents: async (eventName: string, limit = 200): Promise<unknown[]> => {
    const res = await apiV1Fetch<{ ok: boolean; data?: unknown[]; events?: unknown[] }>(
      `/analytics/events?event_name=${encodeURIComponent(eventName)}&limit=${limit}`
    )
    if (Array.isArray(res.data)) return res.data
    if (Array.isArray(res.events)) return res.events
    return []
  },
  /** Batch-retire stale (never-started) tasks in a group. Org admin only. */
  retireStaleTasks: async (
    groupId: string,
    opts: { olderThanDays?: number; batchLimit?: number } = {}
  ): Promise<{ count: number; taskIds: string[] }> => {
    const res = await apiFetch<{ ok: boolean; count?: number; taskIds?: string[] }>(
      `/groups/${encodeURIComponent(groupId)}/tasks/retire-stale`,
      {
        method: 'POST',
        body: JSON.stringify({ olderThanDays: opts.olderThanDays, batchLimit: opts.batchLimit }),
      }
    )
    return { count: res.count ?? 0, taskIds: Array.isArray(res.taskIds) ? res.taskIds : [] }
  },

  listTaskReviewChecks: async (taskId: string): Promise<TaskReviewCheck[]> => {
    const res = await apiFetch<{ ok: boolean; checks: TaskReviewCheck[] }>(
      `/tasks/${encodeURIComponent(taskId)}/review-checks`
    )
    return Array.isArray(res.checks) ? res.checks : []
  },
  setTaskReviewCheck: async (
    taskId: string,
    checkKey: string,
    done: boolean
  ): Promise<TaskReviewCheck> => {
    const res = await apiFetch<{ ok: boolean; check: TaskReviewCheck }>(
      `/tasks/${encodeURIComponent(taskId)}/review-checks/${encodeURIComponent(checkKey)}`,
      { method: 'PATCH', body: JSON.stringify({ done }) }
    )
    return res.check
  },
  readMemoryContent: async (memoryId: string): Promise<MemoryContent> => {
    const res = await apiV1Fetch<{ ok: boolean; data: MemoryContent }>(
      `/context/memory-items/${memoryId}/content`
    )
    return res.data
  },
  recordContextFeedback: async (
    feedback: RecordContextFeedbackRequest
  ): Promise<ContextFeedbackOutcome> => {
    const res = await apiV1Fetch<{ ok: boolean; data: ContextFeedbackOutcome }>(
      '/context/feedback',
      {
        method: 'POST',
        body: JSON.stringify(feedback),
      }
    )
    return res.data
  },
  listContextCandidates: async (
    params: ListContextCandidatesParams = {}
  ): Promise<ContextCandidateSummary[]> => {
    const search = new URLSearchParams()
    if (params.state) search.set('state', params.state)
    if (params.itemKind) search.set('item_kind', params.itemKind)
    if (params.scopeKind) search.set('scope_kind', params.scopeKind)
    if (params.limit) search.set('limit', String(params.limit))
    if (params.offset) search.set('offset', String(params.offset))
    const qs = search.toString()
    const res = await apiV1Fetch<{ ok: boolean; data: ContextCandidateSummary[] }>(
      `/context/candidates${qs ? `?${qs}` : ''}`
    )
    return res.data
  },
  fetchContextFeatures: async (): Promise<ContextFeatureSnapshot> => {
    const res = await apiV1Fetch<{ ok: boolean; data: ContextFeatureSnapshot }>('/context/features')
    return res.data
  },
  listRecurringTasks: async (): Promise<RecurringTask[]> => {
    const res = await apiV1Fetch<{ ok: boolean; data: RecurringTask[] }>('/recurring-tasks')
    return res.data
  },
  createRecurringTask: async (input: CreateRecurringTaskInput): Promise<RecurringTask> => {
    const res = await apiV1Fetch<{ ok: boolean; data: RecurringTask }>('/recurring-tasks', {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return res.data
  },
  updateRecurringTask: async (recurringId: string, enabled: boolean): Promise<RecurringTask> => {
    const res = await apiV1Fetch<{ ok: boolean; data: RecurringTask }>(
      `/recurring-tasks/${encodeURIComponent(recurringId)}`,
      { method: 'PATCH', body: JSON.stringify({ enabled }) }
    )
    return res.data
  },
  deleteRecurringTask: async (recurringId: string): Promise<void> => {
    await apiV1Fetch<{ ok: boolean }>(`/recurring-tasks/${encodeURIComponent(recurringId)}`, {
      method: 'DELETE',
    })
  },

  fetchTaskReviewGates: async (taskId: string): Promise<ReviewGateStatus> => {
    const res = await apiFetch<{ ok: boolean; gates: ReviewGateStatus }>(
      `/tasks/${encodeURIComponent(taskId)}/review-gates`
    )
    return res.gates
  },

  listTaskTemplates: async (params: { projectId?: string } = {}): Promise<TaskTemplate[]> => {
    const qs = params.projectId ? `?projectId=${encodeURIComponent(params.projectId)}` : ''
    const res = await apiV1Fetch<{ ok: boolean; data: TaskTemplate[] }>(`/task-templates${qs}`)
    return res.data
  },
  createTaskTemplate: async (input: CreateTaskTemplateInput): Promise<TaskTemplate> => {
    const res = await apiV1Fetch<{ ok: boolean; data: TaskTemplate }>('/task-templates', {
      method: 'POST',
      body: JSON.stringify(input),
    })
    return res.data
  },
  deleteTaskTemplate: async (templateId: string): Promise<void> => {
    await apiV1Fetch<{ ok: boolean }>(`/task-templates/${encodeURIComponent(templateId)}`, {
      method: 'DELETE',
    })
  },

  fetchAgentUsage: async (params: { hours?: number } = {}): Promise<AgentUsageReport> => {
    const search = new URLSearchParams()
    if (params.hours) search.set('hours', String(params.hours))
    const qs = search.toString()
    const res = await apiV1Fetch<{ ok: boolean; data: AgentUsageReport }>(
      `/analytics/agent-usage${qs ? `?${qs}` : ''}`
    )
    return res.data
  },

  fetchAgentReliability: async (
    params: { hours?: number } = {}
  ): Promise<AgentReliabilityReport> => {
    const search = new URLSearchParams()
    if (params.hours) search.set('hours', String(params.hours))
    const qs = search.toString()
    const res = await apiV1Fetch<{ ok: boolean; data: AgentReliabilityReport }>(
      `/analytics/agent-reliability${qs ? `?${qs}` : ''}`
    )
    return res.data
  },

  fetchContextUsageAnalytics: async (
    params: {
      limit?: number
      minApplied?: number
      staleAfterDays?: number
      minSuccessRate?: number
      negativeRate?: number
    } = {}
  ): Promise<ContextUsageAnalytics> => {
    const search = new URLSearchParams()
    if (params.limit) search.set('limit', String(params.limit))
    if (params.minApplied) search.set('min_applied', String(params.minApplied))
    if (params.staleAfterDays) search.set('stale_after_days', String(params.staleAfterDays))
    if (params.minSuccessRate !== undefined) {
      search.set('min_success_rate', String(params.minSuccessRate))
    }
    if (params.negativeRate !== undefined) search.set('negative_rate', String(params.negativeRate))
    const qs = search.toString()
    const res = await apiV1Fetch<{ ok: boolean; data: ContextUsageAnalytics }>(
      `/analytics/context-usage${qs ? `?${qs}` : ''}`
    )
    return res.data
  },
  fetchGovernanceAudit: async (
    params: GovernanceAuditQueryParams = {}
  ): Promise<GovernanceAuditResponse> => {
    const search = governanceAuditSearchParams(params)
    const qs = search.toString()
    const res = await apiV1Fetch<{ ok: boolean; data: GovernanceAuditResponse }>(
      `/governance/audit${qs ? `?${qs}` : ''}`
    )
    return res.data
  },
  fetchInboxNotifications: async (limit = 50): Promise<InboxNotificationDto[]> => {
    const res = await apiV1Fetch<{ ok: boolean; data: InboxNotificationDto[] }>(
      `/inbox/notifications?limit=${limit}`
    )
    return res.data
  },
  markInboxNotificationRead: (id: string): Promise<{ ok: boolean }> =>
    apiV1Fetch<{ ok: boolean }>(`/inbox/notifications/${encodeURIComponent(id)}/read`, {
      method: 'POST',
    }),
  markAllInboxNotificationsRead: (): Promise<{ ok: boolean }> =>
    apiV1Fetch<{ ok: boolean }>('/inbox/notifications/read-all', { method: 'POST' }),
  exportGovernanceAudit: async (
    params: GovernanceAuditQueryParams = {}
  ): Promise<GovernanceAuditResponse> => {
    const res = await apiV1Fetch<{ ok: boolean; data: GovernanceAuditResponse }>(
      '/governance/audit/export',
      {
        method: 'POST',
        body: JSON.stringify(params),
      }
    )
    return res.data
  },
  approveContextCandidate: async (
    candidateId: string,
    request: ApproveContextCandidateRequest
  ): Promise<ContextApprovalOutcome> => {
    const res = await apiV1Fetch<{ ok: boolean; data: ContextApprovalOutcome }>(
      `/context/candidates/${candidateId}/approve`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    )
    return res.data
  },
  rejectContextCandidate: async (
    candidateId: string,
    request: RejectContextCandidateRequest
  ): Promise<ContextApprovalOutcome> => {
    const res = await apiV1Fetch<{ ok: boolean; data: ContextApprovalOutcome }>(
      `/context/candidates/${candidateId}/reject`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    )
    return res.data
  },
  previewContext: async (taskId: string, agentId: string): Promise<ContextPreviewResponse> => {
    const res = await apiV1Fetch<{ ok: boolean; data: ContextPreviewResponse }>(
      '/context/previews',
      {
        method: 'POST',
        body: JSON.stringify({ taskId, agentId }),
      }
    )
    return res.data
  },
  publishWithContext: async (
    taskId: string,
    request: PublishWithContextRequest
  ): Promise<{ ok: boolean; task: TaskSummary }> =>
    apiFetch<{ ok: boolean; task: TaskSummary }>(`/tasks/${taskId}/publish-with-context`, {
      method: 'POST',
      body: JSON.stringify(request),
    }),
  /**
   * Online participants for the assignment dropdown. Filters to available by default
   * so the modal only offers agents that can actually pick the task up.
   */
  getParticipants: async (
    status: 'available' | 'busy' | 'offline' | 'all' = 'all'
  ): Promise<ParticipantSummary[]> => {
    const qs = status === 'all' ? '' : `?status=${status}`
    const res = await apiFetch<{ ok: boolean; participants?: ParticipantSummary[] }>(
      `/participants${qs}`
    )
    // A list accessor must never return undefined: a response that omits
    // `participants` (an empty or partial payload) would otherwise flow into
    // component state and crash list renderers such as AssignmentReadinessPanel.
    return res.participants ?? []
  },
}

/**
 * Best-effort product analytics beacon (no-op on failure).
 * Named facade over `orchestrationApi.trackProductEvent` so feature components
 * can import one function without dragging the whole client into their module.
 */
export function trackProductEvent(
  eventName: string,
  properties: Record<string, unknown> = {}
): Promise<void> {
  return orchestrationApi.trackProductEvent(eventName, properties)
}

function governanceAuditSearchParams(params: GovernanceAuditQueryParams): URLSearchParams {
  const search = new URLSearchParams()
  if (params.eventType) search.set('eventType', params.eventType)
  if (params.eventPrefix) search.set('eventPrefix', params.eventPrefix)
  if (params.itemKind) search.set('itemKind', params.itemKind)
  if (params.scopeKind) search.set('scopeKind', params.scopeKind)
  if (params.scopeId) search.set('scopeId', params.scopeId)
  if (params.userId) search.set('userId', params.userId)
  if (params.from) search.set('from', params.from)
  if (params.to) search.set('to', params.to)
  if (params.redactSecrets !== undefined) search.set('redactSecrets', String(params.redactSecrets))
  if (params.limit) search.set('limit', String(params.limit))
  if (params.offset) search.set('offset', String(params.offset))
  return search
}
