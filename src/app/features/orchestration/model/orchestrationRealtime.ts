import { useFeedStore } from '@app/entities/feed'
import { useWorkflowStore } from '@app/features/orchestration/model/workflowStore'

/**
 * Realtime events relayed from the orchestrator's `Broadcaster` over NATS
 * `broadcast.{org_id}` and forwarded verbatim by the Rust API on the existing
 * `/ws` socket. These arrive as `{ type, orgId, payload }`; the dispatch hook
 * strips `orgId` (the socket is already org-scoped) and hands us `msg.payload`.
 *
 * Mirrors `features/context/model/contextRealtime.ts`.
 */

export interface WorkflowStatusEvent {
  type: 'workflow:status'
  payload: {
    workflowId: string
    status: string
  }
}

export interface WorkflowNodeStatusEvent {
  type: 'workflow:node_status'
  payload: {
    nodeId: string
    nodeName: string
    status: string
    detail?: string
  }
}

export interface ReviewEscalatedEvent {
  type: 'review.escalated'
  payload: {
    reviewId: string
    taskId: string
    dueAt?: string | null
    overdueSecs?: number | null
  }
}

export type OrchestrationRealtimeEventType =
  | WorkflowStatusEvent['type']
  | WorkflowNodeStatusEvent['type']
  | ReviewEscalatedEvent['type']

export interface OrchestrationRealtimeMessage {
  type: OrchestrationRealtimeEventType
  payload?: unknown
  [key: string]: unknown
}

interface OrchestrationHandlerOptions {
  dev?: boolean
}

export function handleOrchestrationWsMessage(
  message: OrchestrationRealtimeMessage,
  options: OrchestrationHandlerOptions = {}
): void {
  const payload = recordField(message.payload)

  switch (message.type) {
    case 'workflow:status':
      applyWorkflowStatus(payload, options)
      return
    case 'workflow:node_status':
      applyWorkflowNodeStatus(payload, options)
      return
    case 'review.escalated':
      applyReviewEscalated(payload, options)
  }
}

function applyWorkflowStatus(
  payload: Record<string, unknown> | null,
  options: OrchestrationHandlerOptions
) {
  const workflowId = stringField(payload?.workflowId ?? payload?.workflow_id)
  const status = stringField(payload?.status)
  if (!workflowId || !status) {
    logMalformed('workflow:status missing workflowId or status', options)
    return
  }

  useFeedStore.getState().addFeedItem({
    id: `workflow-status:${workflowId}:${status}:${Date.now()}`,
    type: 'workflow_status',
    agentName: '',
    taskTitle: `Background work ${shortId(workflowId)} ${workflowStatusLabel(status)}`,
    detail: workflowStatusDetail(status),
    timestamp: Date.now(),
  })
}

function applyWorkflowNodeStatus(
  payload: Record<string, unknown> | null,
  options: OrchestrationHandlerOptions
) {
  // node_status events carry a nodeId scoped to its parent workflow. The relay
  // payload does not always include the workflowId, so fall back to a stable
  // grouping key when absent rather than dropping the update.
  const nodeId = stringField(payload?.nodeId ?? payload?.node_id)
  if (!nodeId) {
    logMalformed('workflow:node_status missing nodeId', options)
    return
  }
  const workflowId = stringField(payload?.workflowId ?? payload?.workflow_id) ?? 'unknown'
  const nodeName = stringField(payload?.nodeName ?? payload?.node_name) ?? nodeId
  const status = stringField(payload?.status) ?? 'unknown'
  const detail = stringField(payload?.detail)

  useWorkflowStore.getState().upsertNodeStatus(workflowId, nodeId, {
    name: nodeName,
    status,
    detail,
  })
}

function applyReviewEscalated(
  payload: Record<string, unknown> | null,
  options: OrchestrationHandlerOptions
) {
  const reviewId = stringField(payload?.reviewId ?? payload?.review_id)
  if (!reviewId) {
    // review.escalated is the highest-value alert and is rare by construction,
    // so a malformed one must leave a breadcrumb even in production — otherwise
    // an overdue review is dropped with no trace. Logged unconditionally.
    logMalformed('review.escalated missing reviewId', options, { critical: true })
    return
  }
  const taskId = stringField(payload?.taskId ?? payload?.task_id)
  const overdueSecs = numberField(payload?.overdueSecs ?? payload?.overdue_secs)

  // Reuse the same notification surface as task-owner / credential alerts. An
  // overdue review is the highest-value signal — it needs human attention.
  // addNotification dedups by id, so a redelivered escalation updates in place.
  useFeedStore.getState().addNotification({
    id: `review-escalated:${reviewId}`,
    type: 'review_escalated',
    taskId: taskId ?? `review:${reviewId}`,
    taskTitle: 'A review is overdue and needs attention',
    message: overdueReviewMessage(overdueSecs),
    taskHref: '/tasks',
    read: false,
    timestamp: Date.now(),
  })
}

function workflowStatusLabel(status: string): string {
  switch (status.toLowerCase()) {
    case 'completed':
    case 'succeeded':
      return 'completed'
    case 'failed':
    case 'error':
      return 'failed'
    case 'running':
    case 'started':
      return 'is running'
    case 'canceled':
    case 'cancelled':
      return 'was canceled'
    default:
      return status
  }
}

function workflowStatusDetail(status: string): string {
  switch (status.toLowerCase()) {
    case 'completed':
    case 'succeeded':
      return 'The background work finished. Open the related tasks to review the result.'
    case 'failed':
    case 'error':
      return 'The background work stopped before finishing. Open the related tasks to see what is needed.'
    case 'canceled':
    case 'cancelled':
      return 'The background work was canceled before finishing.'
    default:
      return 'The background work shared a status update.'
  }
}

function overdueReviewMessage(overdueSecs: number | null): string {
  if (overdueSecs && overdueSecs > 0) {
    return `Approve or reject this review in your task list so work can continue. It is overdue by about ${overdueDuration(overdueSecs)}.`
  }
  return 'Approve or reject this review in your task list so work can continue. It is past its review deadline.'
}

function overdueDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600)
  if (hours >= 24) {
    const days = Math.floor(hours / 24)
    return `${days} day${days === 1 ? '' : 's'}`
  }
  if (hours >= 1) {
    return `${hours} hour${hours === 1 ? '' : 's'}`
  }
  const minutes = Math.max(1, Math.floor(seconds / 60))
  return `${minutes} minute${minutes === 1 ? '' : 's'}`
}

function shortId(id: string): string {
  return id.length > 8 ? id.slice(0, 8) : id
}

function recordField(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function stringField(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined
}

function numberField(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function logMalformed(
  message: string,
  options: OrchestrationHandlerOptions,
  { critical = false }: { critical?: boolean } = {}
) {
  // Critical (action-bearing, rare) events always leave a breadcrumb so a
  // dropped alert is observable in production. High-frequency events stay
  // DEV-only to avoid console spam.
  if (critical) {
    console.warn(`[orchestrationRealtime] ${message}`)
    return
  }
  if (options.dev ?? import.meta.env.DEV) {
    console.error(`[orchestrationRealtime] ${message}`)
  }
}
