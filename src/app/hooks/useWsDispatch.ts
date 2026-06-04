import { useEffect } from 'react'
import type { TaskSummary } from '@app/shared/api/orchestration'
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
          taskTitle: tool ?? eventType,
          detail: tool ? `Tool: ${tool}` : eventType,
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
      ? (task.blockedHint ?? task.blockedReason ?? task.error ?? 'No unblock reason was provided')
      : notificationType === 'failed'
        ? (task.error ?? 'No failure reason was provided')
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
      return `${actor} failed to complete this task: ${detail}`
    case 'completed':
      return `${actor} completed this task: ${detail}`
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
    taskTitle: `${displayName} credential expired`,
    message: `Reconnect ${displayName} auth in Settings before starting new container agents.`,
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
  if (!tool || (state !== 'updated' && state !== 'failed')) return

  const localDigest = stringField(payload.localDigest)
  const remoteDigest = stringField(payload.remoteDigest)
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
    lastError,
    unix,
  })

  const display = displayCliTool(tool)
  useFeedStore.getState().addNotification({
    id: eventId,
    type: 'cli_image_updated',
    taskId: `cli-image:${tool}`,
    taskTitle:
      state === 'updated' ? `${display} agent image updated` : `${display} image check failed`,
    message:
      state === 'updated'
        ? `New ${display} agents will start on the latest CLI. Running agents are unaffected.`
        : `The ${display} image check failed${lastError ? `: ${lastError}` : ''}. New agents keep the current image until it succeeds.`,
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
  if (typeof result.message === 'string' && result.message.trim()) return result.message
  if (typeof result.stdout === 'string' && result.stdout.trim()) {
    return result.stdout.trim().split('\n')[0]?.slice(0, 140) || 'Completed'
  }
  return 'Completed'
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
