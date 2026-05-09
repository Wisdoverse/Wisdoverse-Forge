import type { TaskContextCounts } from '@app/shared/api/orchestration'
import { useBoardStore } from '@app/shared/model/board.store'
import { useContextStore } from '@app/shared/model/context.store'

export type ContextRealtimeEventType =
  | 'context_candidate.created'
  | 'context_candidate.approved'
  | 'context_candidate.rejected'
  | 'context_application.recorded'
  | 'context_feedback.submitted'

export interface ContextRealtimeMessage {
  type: ContextRealtimeEventType
  payload?: unknown
  [key: string]: unknown
}

interface ContextHandlerOptions {
  dev?: boolean
}

const MAX_SEEN_EVENTS = 512
const seenEventIds: string[] = []
const seenEventIdSet = new Set<string>()

export function handleContextWsMessage(
  message: ContextRealtimeMessage,
  options: ContextHandlerOptions = {}
): void {
  const payload = objectPayload(message)
  if (!trackEventOnce(message, payload)) return

  switch (message.type) {
    case 'context_candidate.created':
      useContextStore.getState().incrementPendingCandidateCount()
      return
    case 'context_candidate.approved':
    case 'context_candidate.rejected':
      useContextStore.getState().decrementPendingCandidateCount()
      return
    case 'context_application.recorded':
      applyContextApplication(payload, options)
      return
    case 'context_feedback.submitted':
      applyContextFeedback(payload, options)
  }
}

export function resetContextHandlerDedupeForTests(): void {
  seenEventIds.length = 0
  seenEventIdSet.clear()
}

function applyContextApplication(payload: Record<string, unknown>, options: ContextHandlerOptions) {
  const taskId = stringField(payload.taskId ?? payload.task_id)
  if (!taskId) {
    logMalformed('context_application.recorded missing taskId', options)
    return
  }

  const groupId = stringField(payload.groupId ?? payload.group_id)
  const counts = contextCountsFromPayload(payload)
  if (counts) {
    useBoardStore.getState().updateTaskContextCounts(taskId, counts, { groupId })
    return
  }

  const itemKind = itemKindField(payload.itemKind ?? payload.item_kind)
  if (!itemKind) {
    logMalformed('context_application.recorded missing contextCounts or itemKind', options)
    return
  }
  useBoardStore.getState().incrementTaskContextCounts(taskId, itemKind, { groupId })
}

function applyContextFeedback(payload: Record<string, unknown>, options: ContextHandlerOptions) {
  const taskId = stringField(payload.taskId ?? payload.task_id)
  if (!taskId) {
    logMalformed('context_feedback.submitted missing taskId', options)
    return
  }

  const counts = contextCountsFromPayload(payload)
  if (counts) {
    useBoardStore.getState().updateTaskContextCounts(taskId, counts, {
      groupId: stringField(payload.groupId ?? payload.group_id),
    })
  }
}

function contextCountsFromPayload(payload: Record<string, unknown>): TaskContextCounts | null {
  const nested = objectField(
    payload.contextCounts ?? payload.context_counts ?? payload.appliedCounts
  )
  const source = nested ?? payload
  const appliedMemories = countField(
    source.appliedMemories ?? source.applied_memories ?? source.memoryCount ?? source.memory_count
  )
  const appliedSkills = countField(
    source.appliedSkills ?? source.applied_skills ?? source.skillCount ?? source.skill_count
  )
  const total = countField(source.total ?? source.appliedTotal ?? source.applied_total)
  if (appliedMemories === null && appliedSkills === null && total === null) return null
  const memories = appliedMemories ?? 0
  const skills = appliedSkills ?? 0
  return {
    appliedMemories: memories,
    appliedSkills: skills,
    total: total ?? memories + skills,
  }
}

function trackEventOnce(
  message: ContextRealtimeMessage,
  payload: Record<string, unknown>
): boolean {
  const eventId =
    stringField(payload.eventId ?? payload.event_id ?? payload.id ?? message.eventId) ??
    fallbackEventKey(message, payload)
  if (!eventId) return true
  if (seenEventIdSet.has(eventId)) return false

  seenEventIdSet.add(eventId)
  seenEventIds.push(eventId)
  while (seenEventIds.length > MAX_SEEN_EVENTS) {
    const stale = seenEventIds.shift()
    if (stale) seenEventIdSet.delete(stale)
  }
  return true
}

function fallbackEventKey(
  message: ContextRealtimeMessage,
  payload: Record<string, unknown>
): string | undefined {
  const candidateId = stringField(
    payload.candidateId ?? payload.candidate_id ?? message.candidateId
  )
  if (candidateId) return `${message.type}:${candidateId}`
  return undefined
}

function objectPayload(message: ContextRealtimeMessage): Record<string, unknown> {
  const nested = objectField(message.payload)
  if (nested) return nested
  const { type: _type, payload: _payload, ...rest } = message
  return rest
}

function objectField(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null
}

function stringField(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined
}

function itemKindField(value: unknown): 'memory' | 'skill' | null {
  return value === 'memory' || value === 'skill' ? value : null
}

function countField(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null
  return Math.max(0, Math.trunc(value))
}

function logMalformed(message: string, options: ContextHandlerOptions) {
  if (options.dev ?? import.meta.env.DEV) {
    console.error(`[contextHandlers] ${message}`)
  }
}
