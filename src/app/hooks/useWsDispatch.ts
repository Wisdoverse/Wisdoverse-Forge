import { useEffect } from 'react'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { useAdminStore } from '@app/shared/model/admin.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useFeedStore } from '@app/shared/model/feed.store'
import { useWebSocket } from '@app/shared/model/websocket.context'
import {
  handleContextWsMessage,
  type ContextRealtimeMessage,
} from '@app/features/context/model/contextRealtime'

interface WsMessage {
  type: string
  [key: string]: unknown
}

export function dispatchWsMessage(msg: WsMessage) {
  const payload = msg.payload
  const payloadRecord = recordField(payload)

  switch (msg.type) {
    case 'orchestration:task_update': {
      const task = recordField(payloadRecord?.task) as TaskSummary | null
      if (task) {
        notifyTaskOwner(task)

        const selectedGroupId = useBoardStore.getState().selectedGroupId
        if (selectedGroupId && task.groupId === selectedGroupId) {
          useBoardStore.getState().upsertTask(task)
        }
      }
      break
    }

    case 'orchestration:participant_update': {
      const participant = recordField(payloadRecord?.participant)
      if (participant) {
        const current = useFeedStore.getState().agents
        const mapped = {
          id: stringField(participant.agentId) ?? stringField(participant.id) ?? '',
          name: stringField(participant.name) ?? '',
          status: mapAgentStatus(participant.status),
        }
        const exists = current.find((a) => a.id === mapped.id)
        if (exists) {
          useFeedStore.getState().setAgents(current.map((a) => (a.id === mapped.id ? mapped : a)))
        } else {
          useFeedStore.getState().setAgents([...current, mapped])
        }
      }
      break
    }

    case 'agents': {
      if (Array.isArray(payload)) {
        useFeedStore.getState().setAgents(
          payload.flatMap((agent) => {
            const agentRecord = recordField(agent)
            if (!agentRecord) return []
            return [
              {
                id: stringField(agentRecord.id) ?? '',
                name: stringField(agentRecord.name) ?? '',
                status: mapAgentStatus(agentRecord.status),
              },
            ]
          })
        )
      }
      break
    }

    case 'agent_update': {
      const agent = recordField(payload)
      if (agent) {
        const current = useFeedStore.getState().agents
        const agentId = stringField(agent.id) ?? ''
        const agentName = stringField(agent.name) ?? ''
        const exists = current.find((a) => a.id === agentId)
        if (exists) {
          useFeedStore
            .getState()
            .setAgents(
              current.map((a) =>
                a.id === agentId ? { ...a, status: mapAgentStatus(agent.status) } : a
              )
            )
        } else {
          useFeedStore
            .getState()
            .setAgents([
              ...current,
              { id: agentId, name: agentName, status: mapAgentStatus(agent.status) },
            ])
        }
      }
      break
    }

    case 'event': {
      const evt = recordField(payload)
      if (evt) {
        const eventType = stringField(evt.type) ?? 'event'
        const agentName = stringField(evt.agentName) ?? ''
        const tool = stringField(evt.tool)
        const timestamp = numberField(evt.timestamp) ?? Date.now()

        // Issue #34: streaming LLM tokens are rendered in ChatView, not the feed.
        // Excluding them here keeps the activity feed focused on real lifecycle.
        if (eventType === 'text_stream') break

        useFeedStore.getState().addFeedItem({
          id: `${eventType}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
          type: eventType,
          agentName,
          taskTitle: agentActivityTitle(eventType, tool),
          detail: agentActivityDetail(eventType, tool),
          timestamp,
        })

        if (eventType === 'permission_prompt' || eventType === 'blocked') {
          useFeedStore.getState().addAttentionItem({
            id: `attention-${Date.now()}`,
            taskTitle: tool ?? 'Task',
            agentName,
            reason: eventType === 'permission_prompt' ? 'Permission required' : 'Blocked',
            timestamp: Date.now(),
          })
        }
      }
      break
    }

    case 'credential:status_update': {
      notifyCredentialOwner(payloadRecord)
      break
    }

    case 'cli_image.updated': {
      handleCliImageUpdate(payloadRecord)
      break
    }

    case 'context_candidate.created': {
      handleContextWsMessage(msg as ContextRealtimeMessage)
      break
    }

    case 'context_candidate.approved':
    case 'context_candidate.rejected': {
      handleContextWsMessage(msg as ContextRealtimeMessage)
      break
    }

    case 'context_application.recorded':
    case 'context_feedback.submitted': {
      handleContextWsMessage(msg as ContextRealtimeMessage)
      break
    }

    default:
      break
  }
}

function stringField(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value : null
}

function numberField(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function recordField(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function currentUserId(): string | null {
  try {
    const raw = localStorage.getItem('af:auth:user')
    if (!raw) return null
    const user = JSON.parse(raw) as { id?: unknown }
    return stringField(user.id)
  } catch {
    return null
  }
}

function taskOwnerId(task: TaskSummary): string | null {
  const legacyTask = task as TaskSummary & { created_by?: unknown; ownerUserId?: unknown }
  const owner = task.createdBy ?? legacyTask.created_by ?? legacyTask.ownerUserId
  return stringField(owner)
}

function notifyTaskOwner(task: TaskSummary) {
  const state = String(task.state).toLowerCase()
  const notificationType =
    state === 'blocked'
      ? 'blocked'
      : state === 'failed' || state === 'error'
        ? 'failed'
        : state === 'completed' || state === 'done'
          ? 'completed'
          : null
  const ownerId = taskOwnerId(task)
  if (!notificationType || !task.id || !ownerId) return

  if (ownerId !== currentUserId()) return

  const assigned = task.assignedAgentName ?? task.assignedTo
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
  const actor = assigned || 'Assigned agent'
  switch (type) {
    case 'blocked':
      return `${actor} is blocked and needs owner input: ${detail}`
    case 'failed':
      return `${actor} stopped before finishing. Open the task, review the recovery note, then retry or reassign when ready. ${detail}`
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
      return 'Review the request before the agent continues.'
    case 'blocked':
      return 'Open the task to see what is needed before work can continue.'
    default:
      return 'The agent reported a task update.'
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

function notifyCredentialOwner(payload: Record<string, unknown> | null) {
  if (!payload) return
  const credential = recordField(payload.credential) ?? payload
  const ownerId = stringField(credential.ownerUserId ?? credential.userId)
  if (!ownerId || ownerId !== currentUserId()) return

  const action = stringField(payload.action)?.toLowerCase() ?? ''
  const status = stringField(credential.status)?.toLowerCase() ?? ''
  const reason = stringField(credential.reason ?? credential.revokeReason)
  const isExpired =
    action === 'credential.revoked' ||
    status === 'expired' ||
    status === 'revoked' ||
    reason === 'invalid_grant'
  if (!isExpired) return

  const cliTool =
    stringField(credential.cliTool ?? credential.provider ?? credential.tool) ?? 'container-cli'
  const displayName = displayCliTool(cliTool)
  const revokedAt = stringField(credential.revokedAt ?? credential.updatedAt)
  const revokedTimestamp = revokedAt ? Date.parse(revokedAt) : Number.NaN
  const timestamp = Number.isFinite(revokedTimestamp) ? revokedTimestamp : Date.now()
  const eventKey =
    stringField(payload.eventId ?? credential.eventId) ??
    (Number.isFinite(revokedTimestamp) ? new Date(timestamp).toISOString() : String(timestamp))

  useFeedStore.getState().addNotification({
    id: `credential-owner:${ownerId}:${cliTool}:expired:${eventKey}`,
    type: 'credential_expired',
    taskId: `credential:${cliTool}`,
    taskTitle: `${displayName} account needs reconnecting`,
    message: `Reconnect the ${displayName} account in Settings before starting new managed workspace agents.`,
    taskHref: '/settings',
    ownerUserId: ownerId,
    read: false,
    timestamp,
  })
}

function handleCliImageUpdate(payload: Record<string, unknown> | null) {
  if (!payload) return
  const tool = stringField(payload.tool)
  const state = stringField(payload.state)
  if (!tool || (state !== 'updated' && state !== 'failed' && state !== 'update_available')) return

  const localDigest = stringField(payload.localDigest)
  const remoteDigest = stringField(payload.remoteDigest)
  const localVersion = stringField(payload.localVersion)
  const remoteVersion = stringField(payload.remoteVersion)
  const lastError = stringField(payload.lastError)
  const unix = numberField(payload.unix) ?? Math.floor(Date.now() / 1000)
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
        : `${display} tool package check failed`
  const message =
    state === 'updated'
      ? `New ${display} agents will start on the latest tool package. Running agents are unaffected.`
      : state === 'update_available'
        ? `A newer ${display} tool package is available. Build it from Admin, then new agents can use it. Running agents are unaffected.`
        : `The ${display} tool package check failed. Open Admin and choose Check now after a few minutes, or ask an owner to check tool package access. New agents keep the current tool package until it succeeds.`
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
  if (!result) return 'No completion summary was provided'
  if (Array.isArray(result)) {
    return `${result.length} result artifact${result.length === 1 ? '' : 's'}`
  }
  if (typeof result.message === 'string' && result.message.trim()) {
    return safeCompletionMessage(result.message)
  }
  if (typeof result.stdout === 'string' && result.stdout.trim()) {
    return 'Finished with a text result. Open details to review it.'
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
    lower.includes('panic') ||
    lower.includes('stack trace') ||
    lower.includes('traceback') ||
    lower.includes('exception') ||
    lower.includes('database') ||
    lower.includes('raw command output')

  if (looksLikeSupportDetail) {
    return 'Finished with a summary that needs review. Open details before using the result.'
  }

  return trimmed
}

function mapAgentStatus(status: unknown): 'working' | 'idle' | 'blocked' | 'offline' {
  switch (stringField(status)) {
    case 'working':
    case 'busy':
      return 'working'
    case 'blocked':
      return 'blocked'
    case 'offline':
    case 'error':
      return 'offline'
    default:
      return 'idle'
  }
}

export function useWsDispatch() {
  const { subscribe } = useWebSocket()

  useEffect(
    () =>
      subscribe((data) => {
        if (!data || typeof data !== 'object') return
        dispatchWsMessage(data as WsMessage)
      }),
    [subscribe]
  )
}
