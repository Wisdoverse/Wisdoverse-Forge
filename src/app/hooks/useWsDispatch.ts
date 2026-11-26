import { useEffect } from 'react'
import type { InboundServerMessage, TaskSummary } from '@shared/types'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { useAdminStore } from '@app/entities/admin'
import { useBoardStore } from '@app/entities/navigation'
import { useFeedStore } from '@app/entities/feed'
import { useWebSocket } from '@app/shared/model/websocket.context'
import { handleContextWsMessage } from '@app/features/context'
import { CLONE_STATUS_WS_TYPE, handleCloneStatusWsMessage } from '@app/features/manage-project'
import { handleOrchestrationWsMessage } from '@app/features/orchestration'
import { decodeServerMessage } from './decodeServerMessage'

// F072: monotonic counter so two attention items created in the same millisecond
// get distinct ids. A bare `attention-${Date.now()}` collides under a burst, and
// since `removeAttentionItem` filters by id, dismissing one would remove both.
let attentionSeq = 0
function nextAttentionId(): string {
  attentionSeq += 1
  return `attention-${Date.now()}-${attentionSeq}`
}

/** What `dispatchWsMessage` did with a frame (asserted by the fixture tests). */
export type WsDispatchOutcome = 'handled' | 'ignored' | 'unknown'

/**
 * Frames this dispatcher deliberately does not act on, with the reason. Every
 * other `InboundServerMessage` member must have a `case` below (compile error
 * otherwise).
 */
export const IGNORED_WS_FRAMES = {
  terminal_output: 'Consumed by the terminal subscriber in features/agents/AgentTerminalTab.tsx.',
  terminal_error: 'Consumed by the terminal subscriber in features/agents/AgentTerminalTab.tsx.',
  turn_invalidate:
    'No consumer yet: ChatView loads turns once (chat.store fetchEvents) and has no live refresh.',
} as const satisfies Partial<Record<InboundServerMessage['type'], string>>

/** Reason the `{ ok: false, error }` gateway control frames are ignored. */
export const IGNORED_CONTROL_FRAME_REASON =
  'No consumer yet: realtime_unavailable / realtime_disconnected are not shown to the user.'

type IgnoredFrame = Extract<InboundServerMessage, { type: keyof typeof IGNORED_WS_FRAMES }>

function isIgnoredFrame(message: InboundServerMessage): message is IgnoredFrame {
  return Object.hasOwn(IGNORED_WS_FRAMES, message.type)
}

// Frames that are not a known message are counted and logged once per key.
const unknownFrameCounts = new Map<string, number>()

export function unknownWsFrameCount(key: string): number {
  return unknownFrameCounts.get(key) ?? 0
}

export function resetWsDispatchStateForTests(): void {
  unknownFrameCounts.clear()
}

function reportUnknown(key: string, detail?: string): WsDispatchOutcome {
  const count = (unknownFrameCounts.get(key) ?? 0) + 1
  unknownFrameCounts.set(key, count)
  if (count === 1) {
    console.warn(`[ws] ignoring unknown realtime frame: ${key}${detail ? ` (${detail})` : ''}`)
  }
  return 'unknown'
}

/** `never` parameter: a member of the union without a `case` fails to compile. */
function unhandledFrame(_frame: never, type: string): WsDispatchOutcome {
  return reportUnknown(type)
}

export function dispatchWsMessage(data: unknown): WsDispatchOutcome {
  const decoded = decodeServerMessage(data)
  if (decoded.kind === 'control') return 'ignored'
  if (decoded.kind === 'unrecognized') return reportUnknown(decoded.key, decoded.detail)
  const msg = decoded.message
  if (isIgnoredFrame(msg)) return 'ignored'
  const type = msg.type

  switch (msg.type) {
    case 'orchestration:task_update': {
      const task = msg.payload.task
      notifyTaskOwner(task)

      const selectedGroupId = useBoardStore.getState().selectedGroupId
      if (selectedGroupId && task.groupId === selectedGroupId) {
        useBoardStore.getState().upsertTask(task)
      }
      return 'handled'
    }

    case 'orchestration:participant_update': {
      const participant = msg.payload.participant
      const current = useFeedStore.getState().agents
      const mapped = {
        id: participant.agentId,
        name: participant.name,
        status: feedStatus(participant.status),
      }
      const exists = current.find((a) => a.id === mapped.id)
      if (exists) {
        useFeedStore.getState().setAgents(current.map((a) => (a.id === mapped.id ? mapped : a)))
      } else {
        useFeedStore.getState().setAgents([...current, mapped])
      }
      return 'handled'
    }

    // MS-3 PR-A: 'agents' / 'agent_update' cases removed — the Rust backend never
    // emits those frames (legacy TypeScript-server types). Agent lists come from
    // REST + the live 'orchestration:participant_update' frame instead.

    case 'event': {
      // The gateway relays events in the flat BroadcastMessage shape
      // ({ type:'event', eventType, eventData, agentId, orgId }) — the event
      // detail (tool, timestamp, …) lives in `eventData`, NOT a nested `payload`.
      const data = msg.eventData
      const eventType = stringField(msg.eventType) ?? 'event'
      // `agentId` is a session id / UUID on the wire, not a display name. Show
      // the CLI tool the relay hook stamps into eventData when present, but the
      // canonical Rust event frame (normalize_event_data / the event.json
      // fixture) carries no cliTool — fall back to the repo's neutral actor
      // label ('The agent', matching HistoryTab) so the feed/attention UI never
      // renders a blank ` is waiting`.
      const agentName = stringField(data.cliTool) ?? 'The agent'
      const tool = stringField(data.tool)
      const timestamp = numberField(data.timestamp) ?? Date.now()

      // Streaming LLM output is rendered in ChatView, not the feed. Both
      // `text_stream` (issue #34) and `token_update` fire on every streamed
      // token and are broadcast-but-not-persisted server-side
      // (is_persistable() in event_consumer.rs), so surfacing them here would
      // flood the activity feed. Drop them to keep the feed on real lifecycle.
      if (eventType === 'text_stream' || eventType === 'token_update') return 'handled'

      useFeedStore.getState().addFeedItem({
        id: `${eventType}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        type: eventType,
        agentName,
        taskTitle: agentActivityTitle(eventType, tool),
        detail: agentActivityDetail(eventType, tool),
        timestamp,
      })

      // The live Rust event type is `permission_request` (the sidecar hook
      // name); `permission_prompt` is the legacy projected alias — accept both.
      const needsPermission =
        eventType === 'permission_request' || eventType === 'permission_prompt'
      if (needsPermission || eventType === 'blocked') {
        useFeedStore.getState().addAttentionItem({
          id: nextAttentionId(),
          taskTitle: tool ?? 'Task',
          agentName,
          reason: needsPermission ? 'Permission required' : 'Blocked',
          timestamp: Date.now(),
        })
      }
      return 'handled'
    }

    case 'credential:status_update': {
      notifyCredentialOwner(msg.payload)
      return 'handled'
    }

    case 'cli_image.updated': {
      handleCliImageUpdate(msg.payload)
      return 'handled'
    }

    case CLONE_STATUS_WS_TYPE: {
      handleCloneStatusWsMessage(msg)
      return 'handled'
    }

    case 'context_candidate.created':
    case 'context_candidate.approved':
    case 'context_candidate.rejected':
    case 'context_application.recorded':
    case 'context_feedback.submitted': {
      handleContextWsMessage(msg)
      return 'handled'
    }

    // Orchestrator realtime events relayed via NATS broadcast.{org_id} and
    // forwarded verbatim by the Rust API on this same socket. `msg.payload`
    // carries the event body; the extra `orgId` is ignored (socket is scoped).
    case 'workflow:status':
    case 'workflow:node_status':
    case 'review.escalated': {
      handleOrchestrationWsMessage(msg)
      return 'handled'
    }

    default:
      return unhandledFrame(msg, type)
  }
}

function stringField(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value : null
}

function numberField(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

// F073: `currentUserId` runs once per dispatched WS message (owner-notification
// gating). Cache the parsed id and only re-`JSON.parse` when the stored string
// actually changes, so a steady stream of broadcasts does not re-parse the user
// blob on every message. The cheap `getItem` still runs to detect a change.
let cachedUserRaw: string | null = null
let cachedUserId: string | null = null
function currentUserId(): string | null {
  try {
    // `getItem` stays inside the guard: storage-restricted browsers throw a
    // SecurityError, and owner-notification gating must fail soft (return null)
    // rather than abort `dispatchWsMessage` and drop the live update.
    const raw = typeof localStorage !== 'undefined' ? localStorage.getItem('af:auth:user') : null
    if (raw === cachedUserRaw) return cachedUserId
    // Parse FIRST, then commit both cache fields only on success — otherwise a
    // valid→malformed transition would leave `cachedUserId` holding the previous
    // user's id while `cachedUserRaw` advanced, so the next same-malformed call
    // would return that stale owner (fail-open) instead of null (fail-closed).
    const id = raw ? stringField((JSON.parse(raw) as { id?: unknown }).id) : null
    cachedUserRaw = raw
    cachedUserId = id
    return id
  } catch {
    return null
  }
}

function notifyTaskOwner(task: TaskSummary) {
  const notificationType =
    task.state === 'blocked' || task.state === 'failed' || task.state === 'completed'
      ? task.state
      : null
  const ownerId = stringField(task.createdBy)
  if (!notificationType || !task.id || !ownerId) return

  if (ownerId !== currentUserId()) return

  const assigned = task.assignedAgentName ?? (task.assignedTo ? 'Chosen agent' : undefined)
  const taskTitle = task.params?.task ?? task.id
  const updatedAt = Date.parse(task.updatedAt)
  const timestamp = Number.isFinite(updatedAt) ? updatedAt : Date.now()
  const detail =
    notificationType === 'blocked'
      ? taskBlockedPreview({
          blockedHint: task.blockedHint,
          blockedReason: task.blockedReason,
          error: task.error,
        })
      : notificationType === 'failed'
        ? taskFailurePreview(task.error)
        : completionSummary(task)

  useFeedStore.getState().addNotification({
    id: `task-owner:${task.id}:${notificationType}`,
    type: notificationType,
    taskId: task.id,
    taskTitle,
    message: taskNotificationMessage(notificationType, assigned, detail),
    taskHref: '/tasks',
    ownerUserId: ownerId,
    read: false,
    timestamp,
  })
}

function taskNotificationMessage(
  type: 'blocked' | 'failed' | 'completed',
  assigned: string | undefined,
  detail: string
): string {
  const actor = assigned || 'Chosen agent'
  switch (type) {
    case 'blocked':
      return `${actor} needs your answer before work can continue: ${detail}`
    case 'failed':
      return `${actor} stopped before finishing. Open the task, read the recovery note, then retry or choose another agent. ${detail}`
    case 'completed':
      return `${actor} completed this task: ${detail}`
  }
}

function agentActivityTitle(eventType: string, tool?: string | null): string {
  switch (eventType) {
    case 'pre_tool_use':
      return tool ? activityToolLabel(tool) : 'Starting a work step'
    case 'post_tool_use':
      return tool ? `Finished ${activityToolLabel(tool).toLowerCase()}` : 'Finished a work step'
    case 'permission_prompt':
    case 'permission_request':
      return 'Decision needed'
    case 'blocked':
      return 'Needs help'
    default:
      return 'Task update'
  }
}

function agentActivityDetail(eventType: string, tool?: string | null): string {
  switch (eventType) {
    case 'pre_tool_use':
      return tool
        ? `Started ${activityToolLabel(tool).toLowerCase()}.`
        : 'The agent started a work step.'
    case 'post_tool_use':
      return tool
        ? `Finished ${activityToolLabel(tool).toLowerCase()}.`
        : 'The agent finished a work step.'
    case 'permission_prompt':
    case 'permission_request':
      return 'Check the request before the agent continues.'
    case 'blocked':
      return 'Open the task to see what is needed before work can continue.'
    default:
      return 'The agent shared a task update.'
  }
}

function activityToolLabel(tool: string): string {
  switch (tool.toLowerCase()) {
    case 'read':
      return 'Checking project files'
    case 'write':
    case 'edit':
    case 'multiedit':
      return 'Updating project files'
    case 'bash':
    case 'shell':
      return 'Running a project command'
    case 'grep':
    case 'glob':
    case 'ls':
    case 'rg':
      return 'Searching the project'
    case 'webfetch':
    case 'websearch':
      return 'Checking online information'
    case 'todowrite':
      return 'Updating the work checklist'
    default:
      return 'Working on the task'
  }
}

type CredentialStatusPayload = Extract<
  InboundServerMessage,
  { type: 'credential:status_update' }
>['payload']
type CliImageUpdatedPayload = Extract<
  InboundServerMessage,
  { type: 'cli_image.updated' }
>['payload']

function notifyCredentialOwner(payload: CredentialStatusPayload) {
  const credential = payload.credential
  const ownerId = stringField(credential.ownerUserId)
  if (!ownerId || ownerId !== currentUserId()) return

  const action = payload.action.toLowerCase()
  const status = credential.status.toLowerCase()
  const reason = stringField(credential.reason)
  const isExpired =
    action === 'credential.revoked' ||
    status === 'expired' ||
    status === 'revoked' ||
    reason === 'invalid_grant'
  if (!isExpired) return

  const cliTool = stringField(credential.cliTool) ?? 'container-cli'
  const displayName = displayCliTool(cliTool)
  const revokedAt = stringField(credential.revokedAt)
  const revokedTimestamp = revokedAt ? Date.parse(revokedAt) : Number.NaN
  const timestamp = Number.isFinite(revokedTimestamp) ? revokedTimestamp : Date.now()
  const eventKey =
    stringField(payload.eventId) ??
    (Number.isFinite(revokedTimestamp) ? new Date(timestamp).toISOString() : String(timestamp))

  useFeedStore.getState().addNotification({
    id: `credential-owner:${ownerId}:${cliTool}:expired:${eventKey}`,
    type: 'credential_expired',
    taskId: `credential:${cliTool}`,
    taskTitle: `${displayName} account needs reconnecting`,
    message: `Open Codex sign-in, then reconnect the ${displayName} account before starting new agents that work on files.`,
    taskHref: '/settings/work-tool-sign-ins',
    ownerUserId: ownerId,
    read: false,
    timestamp,
  })
}

function handleCliImageUpdate(payload: CliImageUpdatedPayload) {
  const { state, unix } = payload
  const tool = stringField(payload.tool)
  if (!tool) return

  const localDigest = stringField(payload.localDigest)
  const remoteDigest = stringField(payload.remoteDigest)
  const localVersion = stringField(payload.localVersion)
  const remoteVersion = stringField(payload.remoteVersion)
  const lastError = stringField(payload.lastError)
  // The producer's stable dedup key; fall back to a derived one. addNotification
  // dedups by id, so a redelivered frame updates in place rather than re-toasting.
  const eventId = stringField(payload.eventId) ?? `cli-image:${tool}:${state}`

  // Live-patch an open admin panel so it reflects the change before the next poll.
  useAdminStore.getState().applyCliImageUpdate({
    tool,
    state,
    localDigest,
    remoteDigest,
    localVersion,
    remoteVersion,
    lastError,
    unix,
  })

  const display = displayCliTool(tool)
  const title =
    state === 'updated'
      ? `${display} agent tool package updated`
      : state === 'update_available'
        ? `${display} update available${remoteVersion ? ` (v${remoteVersion})` : ''}`
        : `Check ${display} tool package in Admin`
  const message =
    state === 'updated'
      ? `New ${display} agents will start on the latest tool package. Running agents are unaffected.`
      : state === 'update_available'
        ? `A newer ${display} tool package is available. Build it from Admin, then new agents can use it. Running agents are unaffected.`
        : `Open Admin and choose Agent tool updates, then choose Check now after a few minutes. New agents keep the current tool package until the check succeeds. If it still fails, ask an owner to check tool package access.`
  useFeedStore.getState().addNotification({
    id: eventId,
    type: 'cli_image_updated',
    taskId: `cli-image:${tool}`,
    taskTitle: title,
    message,
    taskHref: '/admin',
    read: false,
    timestamp: unix * 1000,
  })
}

function displayCliTool(cliTool: string): string {
  switch (cliTool.toLowerCase()) {
    case 'codex':
      return 'Codex'
    case 'claude':
      return 'Claude'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    default:
      return 'Work tool'
  }
}

function completionSummary(task: TaskSummary): string {
  const result = task.result
  if (!result) return 'Open the task details to confirm what changed before using the result.'
  if (Array.isArray(result)) {
    return `${result.length} result file${result.length === 1 ? '' : 's'}`
  }
  if (typeof result === 'object') {
    if ('message' in result && typeof result.message === 'string' && result.message.trim()) {
      return safeCompletionMessage(result.message)
    }
    if ('stdout' in result && typeof result.stdout === 'string' && result.stdout.trim()) {
      return 'Finished with a text result. Open the task details to check it before using it.'
    }
  }
  return 'Completed'
}

function safeCompletionMessage(message: string): string {
  const trimmed = message.trim()
  const lower = trimmed.toLowerCase()
  const looksLikeSupportDetail =
    trimmed.length > 180 ||
    trimmed.includes('\n') ||
    /\b(token|secret|password|api[_\s-]?key|credential|credentials)\b/i.test(trimmed) ||
    /\b(401|403|500|502|503|504)\b/.test(trimmed) ||
    lower.includes('unauthorized') ||
    lower.includes('forbidden') ||
    lower.includes('role required') ||
    lower.includes('panic') ||
    lower.includes('stack trace') ||
    lower.includes('traceback') ||
    lower.includes('exception') ||
    lower.includes('database') ||
    lower.includes('raw command output')

  if (looksLikeSupportDetail) {
    return 'Finished with a summary to check. Open the task details before using the result.'
  }

  return trimmed
}

/** WS participant status -> activity-feed agent status (`online` means idle). */
function feedStatus(
  status: Extract<
    InboundServerMessage,
    { type: 'orchestration:participant_update' }
  >['payload']['participant']['status']
): 'working' | 'idle' | 'offline' {
  switch (status) {
    case 'online':
      return 'idle'
    case 'busy':
      return 'working'
    case 'offline':
      return 'offline'
  }
}

export function useWsDispatch() {
  const { subscribe } = useWebSocket()

  useEffect(
    () =>
      subscribe((data) => {
        dispatchWsMessage(data)
      }),
    [subscribe]
  )
}
