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

export type TaskState =
  | 'backlog'
  | 'queued'
  | 'working'
  | 'blocked'
  | 'completed'
  | 'failed'
  | 'canceled'

export type BlockedReason =
  | 'waiting_agent'
  | 'waiting_dependency'
  | 'waiting_input'
  | 'waiting_approval'
  | 'quota_exceeded'

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
  completedAt?: string
  contextCounts?: TaskContextCounts
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
    return [{ name: 'stdout.txt', mimeType: 'text/plain', data: result.stdout }]
  }

  if (typeof result.message === 'string') {
    return [{ name: 'message.txt', mimeType: 'text/plain', data: result.message }]
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
  const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
  const res = await fetch(`${base}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...(options?.headers instanceof Headers
        ? Object.fromEntries(options.headers.entries())
        : (options?.headers ?? {})),
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
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
  fetchContextForTask: async (taskId: string): Promise<TaskContextResponse> => {
    const res = await apiFetch<{ ok: boolean; data: TaskContextResponse }>(
      `/tasks/${taskId}/context`
    )
    return res.data
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
    const res = await apiFetch<{ ok: boolean; participants: ParticipantSummary[] }>(
      `/participants${qs}`
    )
    return res.participants
  },
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
