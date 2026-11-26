import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { useAdminStore } from '@app/entities/admin'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { useContextStore } from '@app/features/context/model/context.store'
import { useFeedStore } from '@app/entities/feed'
import {
  IGNORED_WS_FRAMES,
  dispatchWsMessage,
  resetWsDispatchStateForTests,
  unknownWsFrameCount,
} from '@app/hooks/useWsDispatch'
import { CONTROL_FRAME_FIXTURES, WS_FRAME_FIXTURES } from '@app/hooks/wsFrameCoverage'
import { useNavigationStore } from '@app/entities/navigation'
import { resetContextHandlerDedupeForTests } from '@app/features/context/model/contextRealtime'

beforeEach(() => {
  useBoardStore.getState().reset()
  useContextStore.getState().reset()
  useFeedStore.getState().reset()
  useAdminStore.setState({ cliImages: null, cliImagesLoading: false, cliImagesError: null })
  resetContextHandlerDedupeForTests()
  resetWsDispatchStateForTests()
  localStorage.clear()
})

// Fields the Rust TaskSummary serializer always sends (ws_protocol.rs).
const WIRE_TASK = {
  createdBy: 'user-other',
  selfFix: false,
  contextCounts: { appliedMemories: 0, appliedSkills: 0, total: 0 },
  attempt: 1,
}

// `Option` fields the cli_image.updated payload always sends, as null.
const CLI_IMAGE_NULLS = {
  localDigest: null,
  remoteDigest: null,
  localVersion: null,
  remoteVersion: null,
  lastError: null,
}

describe('dispatchWsMessage', () => {
  it('dispatches orchestration:task_update to board store when group matches', () => {
    useBoardStore.getState().setSelectedGroupId('g1')

    const task = {
      ...WIRE_TASK,
      id: 'task1',
      groupId: 'g1',
      state: 'working',
      method: 'code',
      params: { task: 'Fix bug', message: '' },
      priority: 'normal' as const,
      progress: 50,
      createdAt: '2026-04-03T00:00:00Z',
      updatedAt: '2026-04-03T00:00:00Z',
    }

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: { action: 'updated', eventId: 'evt-task', task },
    })

    const columns = useBoardStore.getState().columns
    expect(columns.working).toHaveLength(1)
    expect(columns.working[0].id).toBe('task1')
  })

  it('ignores orchestration:task_update for a different group', () => {
    useBoardStore.getState().setSelectedGroupId('g1')

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task2',
          groupId: 'g-other',
          state: 'working',
          method: 'code',
          params: { task: 'X', message: '' },
          priority: 'normal',
          progress: 0,
          createdAt: '',
          updatedAt: '',
        },
        groupId: 'g-other',
      },
    })

    const all = Object.values(useBoardStore.getState().columns).flat()
    expect(all).toHaveLength(0)
  })

  it('ignores the removed legacy agents / agent_update frames (MS-3 PR-A)', () => {
    // These ServerMessage variants were deleted — the Rust backend never emits
    // them — so the dispatcher must now leave the feed store untouched.
    const before = useFeedStore.getState().agents.length
    dispatchWsMessage({
      type: 'agent_update',
      payload: { id: 'a1', name: 'Claude', status: 'working' },
    })
    dispatchWsMessage({
      type: 'agents',
      payload: [{ id: 'a1', name: 'Claude', status: 'idle' }],
    })
    expect(useFeedStore.getState().agents).toHaveLength(before)
  })

  it('dispatches the flat Rust event frame to feed items', () => {
    // The gateway relays events in the flat BroadcastMessage shape
    // ({type,eventType,eventData,agentId,orgId}) — NOT a nested `payload`.
    dispatchWsMessage({
      type: 'event',
      eventType: 'pre_tool_use',
      eventData: { type: 'pre_tool_use', tool: 'Read', timestamp: Date.now() },
      agentId: 'agent-1',
      orgId: 'org-1',
    })

    expect(useFeedStore.getState().feedItems).toHaveLength(1)
    expect(useFeedStore.getState().feedItems[0].type).toBe('pre_tool_use')
    expect(useFeedStore.getState().feedItems[0].taskTitle).toBe('Checking project files')
    expect(useFeedStore.getState().feedItems[0].detail).toBe('Started checking project files.')
    expect(useFeedStore.getState().feedItems[0].detail).not.toContain('Tool:')
    expect(useFeedStore.getState().feedItems[0].taskTitle).not.toBe('Read')
    // The canonical Rust event frame carries no `cliTool` (see the locked
    // event.json fixture / normalize_event_data), so the actor must fall back to
    // a human label — never a blank string that renders as ` is waiting`.
    expect(useFeedStore.getState().feedItems[0].agentName).toBe('The agent')
  })

  it('surfaces a blocked event without cliTool as a named attention item', () => {
    // `blocked` frames from the Rust gateway do not include cliTool. A blank
    // agentName would render as ` is waiting: …` in AttentionZone, so the
    // attention item must still carry a visible actor label.
    dispatchWsMessage({
      type: 'event',
      eventType: 'blocked',
      eventData: { type: 'blocked', tool: 'Bash', timestamp: Date.now() },
      agentId: 'cli-session-xyz',
      orgId: 'org-1',
    })

    const attention = useFeedStore.getState().attentionItems
    expect(attention).toHaveLength(1)
    expect(attention[0].agentName).toBe('The agent')
    expect(attention[0].agentName).not.toBe('')
  })

  it('suppresses high-frequency streaming events from the feed', () => {
    // `token_update` is broadcast on every streamed LLM token but is NOT
    // persisted server-side (is_persistable() in event_consumer.rs). Surfacing
    // it as a feed item would flood the activity feed during streaming, so the
    // dispatcher must drop it just like `text_stream`.
    dispatchWsMessage({
      type: 'event',
      eventType: 'token_update',
      eventData: { type: 'token_update', timestamp: Date.now() },
      agentId: 'agent-1',
      orgId: 'org-1',
    })

    expect(useFeedStore.getState().feedItems).toHaveLength(0)
  })

  it('gives same-millisecond attention items distinct ids (F072)', () => {
    const event = {
      type: 'event' as const,
      eventType: 'permission_prompt',
      eventData: { type: 'permission_prompt', tool: 'Bash', timestamp: 1 },
      agentId: 'Claude',
      orgId: 'org-1',
    }
    dispatchWsMessage(event)
    dispatchWsMessage(event)

    const items = useFeedStore.getState().attentionItems
    expect(items).toHaveLength(2)
    // Distinct ids so dismissing one (removeAttentionItem filters by id) cannot
    // remove the other — a bare `attention-${Date.now()}` would collide.
    expect(items[0].id).not.toBe(items[1].id)
  })

  it('turns command activity events into plain work steps', () => {
    dispatchWsMessage({
      type: 'event',
      eventType: 'post_tool_use',
      eventData: { type: 'post_tool_use', tool: 'Bash', timestamp: Date.now() },
      agentId: 'Codex',
      orgId: 'org-1',
    })

    const item = useFeedStore.getState().feedItems[0]
    expect(item.taskTitle).toBe('Finished running a project command')
    expect(item.detail).toBe('Finished running a project command.')
    expect(item.detail).not.toContain('Tool:')
    expect(item.taskTitle).not.toBe('Bash')
  })

  it('uses plain wording for unknown activity events', () => {
    dispatchWsMessage({
      type: 'event',
      eventType: 'future_event',
      eventData: { type: 'future_event', timestamp: Date.now() },
      agentId: 'Codex',
      orgId: 'org-1',
    })

    const item = useFeedStore.getState().feedItems[0]
    expect(item.taskTitle).toBe('Task update')
    expect(item.detail).toBe('The agent shared a task update.')
    expect(item.detail).not.toContain('reported')
  })

  it('uses check-first wording for permission prompts', () => {
    dispatchWsMessage({
      type: 'event',
      eventType: 'permission_prompt',
      eventData: { type: 'permission_prompt', timestamp: Date.now() },
      agentId: 'Codex',
      orgId: 'org-1',
    })

    const item = useFeedStore.getState().feedItems[0]
    expect(item.taskTitle).toBe('Decision needed')
    expect(item.detail).toBe('Check the request before the agent continues.')
    expect(item.detail).not.toContain('Review the request')
  })

  it('surfaces permission_request events as attention items and shows the CLI tool', () => {
    // `permission_request` is the real Rust event type (permission_prompt is a
    // legacy alias); agentName should be the CLI tool, not the wire session id.
    dispatchWsMessage({
      type: 'event',
      eventType: 'permission_request',
      eventData: {
        type: 'permission_request',
        tool: 'Bash',
        cliTool: 'claude',
        timestamp: Date.now(),
      },
      agentId: 'cli-session-abc',
      orgId: 'org-1',
    })

    const feed = useFeedStore.getState().feedItems[0]
    expect(feed.taskTitle).toBe('Decision needed')
    expect(feed.agentName).toBe('claude')
    const attention = useFeedStore.getState().attentionItems
    expect(attention).toHaveLength(1)
    expect(attention[0].agentName).toBe('claude')
  })

  it('ignores unknown message types', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    expect(dispatchWsMessage({ type: 'pong' })).toBe('unknown')
    expect(dispatchWsMessage({ type: 'unknown_thing' })).toBe('unknown')
    expect(dispatchWsMessage({ type: 'unknown_thing' })).toBe('unknown')
    expect(dispatchWsMessage('not json')).toBe('unknown')
    // Counted every time, logged once per type.
    expect(unknownWsFrameCount('unknown_thing')).toBe(2)
    expect(warn).toHaveBeenCalledTimes(3)
  })

  it('dispatches context_application.recorded to task context counts', () => {
    useBoardStore.getState().setSelectedGroupId('g1')
    useBoardStore.getState().setTasks([
      {
        id: 'task-context-ws',
        groupId: 'g1',
        state: 'working',
        method: 'code',
        params: { task: 'Use approved context', message: '' },
        priority: 'normal',
        progress: 0,
        createdAt: '2026-04-03T00:00:00Z',
        updatedAt: '2026-04-03T00:00:00Z',
      },
    ])

    dispatchWsMessage({
      type: 'context_application.recorded',
      payload: {
        eventId: 'context-ws-1',
        taskId: 'task-context-ws',
        groupId: 'g1',
        contextCounts: { appliedMemories: 1, appliedSkills: 2, total: 3 },
      },
    })

    const all = Object.values(useBoardStore.getState().columns).flat()
    expect(all[0].contextCounts).toEqual({ appliedMemories: 1, appliedSkills: 2, total: 3 })
  })

  it('dispatches context candidate events to the approval queue count', () => {
    // Flat frame, as built by ContextCandidateBroadcast::payload (domain/context.rs).
    const candidate = {
      candidateId: 'candidate-ws-1',
      itemKind: 'memory',
      scopeKind: 'user',
      scopeId: 'user-owner',
      timestamp: '2026-04-03T00:00:00+00:00',
    }
    expect(
      dispatchWsMessage({ type: 'context_candidate.created', state: 'pending', ...candidate })
    ).toBe('handled')
    expect(
      dispatchWsMessage({ type: 'context_candidate.rejected', state: 'rejected', ...candidate })
    ).toBe('handled')

    expect(useContextStore.getState().pendingCandidateCount).toBe(0)
  })

  it('notifies the human task owner when their task becomes blocked', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-1',
          groupId: 'g-other',
          state: 'blocked',
          method: 'code',
          params: { task: 'Deploy staging', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          blockedHint: 'Waiting for SSH approval',
          priority: 'normal',
          progress: 0,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0]).toMatchObject({
      id: 'task-owner:task-owner-1:blocked',
      type: 'blocked',
      taskId: 'task-owner-1',
      taskTitle: 'Deploy staging',
      ownerUserId: 'user-owner',
      taskHref: '/tasks',
      read: false,
    })
    expect(notifications[0].message).toContain(
      'Open the task details to see what needs confirmation, then choose Allow and continue when it is ready.'
    )
    expect(notifications[0].message).not.toContain('Waiting for SSH approval')
    expect(notifications[0].message).toContain('needs your answer before work can continue')
    expect(notifications[0].message).not.toContain('needs owner input')
    expect(notifications[0].message).not.toContain('is blocked')
  })

  it('fails closed on a valid→malformed user blob — no stale-owner notifications (F073)', () => {
    const blocked = (taskId: string) => ({
      type: 'orchestration:task_update' as const,
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: taskId,
          groupId: 'g-other',
          state: 'blocked',
          method: 'code',
          params: { task: 'Deploy', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          blockedHint: 'Waiting',
          priority: 'normal',
          progress: 0,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    // Valid user → owner notified (and the parsed id is cached).
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))
    dispatchWsMessage(blocked('task-a'))
    expect(useFeedStore.getState().notifications).toHaveLength(1)

    // The blob becomes malformed. Two more owned tasks must NOT notify — the
    // cache must not keep serving the previous user's id after a parse failure.
    localStorage.setItem('af:auth:user', '{not json')
    dispatchWsMessage(blocked('task-b'))
    dispatchWsMessage(blocked('task-c'))
    expect(useFeedStore.getState().notifications).toHaveLength(1)
  })

  it('uses chosen-agent wording when a task owner notification has no agent name yet', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-no-agent-name',
          groupId: 'g-other',
          state: 'completed',
          method: 'code',
          params: { task: 'Check copy', message: '' },
          createdBy: 'user-owner',
          assignedTo: 'agent-123',
          result: { message: 'Copy updated' },
          priority: 'normal',
          progress: 100,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain('Chosen agent completed this task')
    expect(notifications[0].message).not.toContain('Assigned agent')
    expect(notifications[0].message).not.toContain('agent-123')
  })

  it('hides raw blocked fallback details in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-blocked-raw',
          groupId: 'g-other',
          state: 'blocked',
          method: 'code',
          params: { task: 'Scale preview worker', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          blockedReason: 'quota_exceeded',
          error: 'quota_exceeded: docker socket denied secret token abc',
          priority: 'normal',
          progress: 0,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain(
      'Pause lower-priority work or ask an owner to raise the limit, then retry.'
    )
    expect(notifications[0].message).not.toContain('quota_exceeded')
    expect(notifications[0].message).not.toContain('docker socket')
    expect(notifications[0].message).not.toContain('secret token')
  })

  it('hides raw blocked error details when no structured reason is available', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-blocked-error',
          groupId: 'g-other',
          state: 'blocked',
          method: 'code',
          params: { task: 'Reconnect account', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          error: '401 Unauthorized: token expired',
          priority: 'normal',
          progress: 0,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain(
      'This task needs account access before it can continue.'
    )
    expect(notifications[0].message).not.toContain('401 Unauthorized')
    expect(notifications[0].message).not.toContain('token expired')
  })

  it('notifies the human task owner when their task fails', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-failed',
          groupId: 'g1',
          state: 'failed',
          method: 'code',
          params: { task: 'Send pong', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          error: '401 Unauthorized',
          priority: 'normal',
          progress: 0,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0]).toMatchObject({
      id: 'task-owner:task-owner-failed:failed',
      type: 'failed',
      taskId: 'task-owner-failed',
      taskTitle: 'Send pong',
      ownerUserId: 'user-owner',
      taskHref: '/tasks',
      read: false,
    })
    expect(notifications[0].message).toContain('Reconnect sign-in or service access, then retry.')
    expect(notifications[0].message).toContain(
      'stopped before finishing. Open the task, read the recovery note, then retry or choose another agent.'
    )
    expect(notifications[0].message).not.toContain('when ready')
    expect(notifications[0].message).not.toContain('reassign')
    expect(notifications[0].message).not.toContain('failed to complete this task')
    expect(notifications[0].message).not.toContain('401 Unauthorized')
  })

  it('reports the retired TypeScript-server task shape as unknown instead of notifying', () => {
    // `created_by` and state 'error' came from the retired TypeScript server. The
    // Rust TaskSummary serializer always sends `createdBy` and only kanban states
    // (rust/crates/core/src/ws_protocol.rs), so this frame no longer decodes.
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const outcome = dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.failed',
        eventId: 'evt-task',
        task: {
          id: 'task-owner-error',
          groupId: 'g1',
          state: 'error',
          method: 'code',
          params: { task: 'Run migration', message: '' },
          created_by: 'user-owner',
          assignedAgentName: 'Codex',
          error: 'migration exited non-zero',
          priority: 'normal',
          progress: 0,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    expect(outcome).toBe('unknown')
    expect(unknownWsFrameCount('orchestration:task_update (malformed)')).toBe(1)
    expect(warn).toHaveBeenCalledWith(
      '[ws] ignoring unknown realtime frame: orchestration:task_update (malformed)'
    )
    expect(useFeedStore.getState().notifications).toHaveLength(0)
  })

  it('does not notify non-owners and does not duplicate repeated completed events', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    const task = {
      ...WIRE_TASK,
      id: 'task-owner-2',
      groupId: 'g1',
      state: 'completed',
      method: 'code',
      params: { task: 'Fix production bug', message: '' },
      createdBy: 'user-owner',
      assignedAgentName: 'Claude',
      result: { message: 'Patch merged' },
      priority: 'normal',
      progress: 100,
      createdAt: '2026-04-03T00:00:00Z',
      updatedAt: '2026-04-03T00:01:00Z',
    }

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: { action: 'task.completed', eventId: 'evt-task', task },
    })
    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: { action: 'task.completed', eventId: 'evt-task', task },
    })
    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.completed',
        eventId: 'evt-task',
        task: { ...task, id: 'task-owner-3', createdBy: 'someone-else' },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].id).toBe('task-owner:task-owner-2:completed')
    expect(notifications[0].message).toContain('Patch merged')
  })

  it('describes completed result arrays as result files in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-result-files',
          groupId: 'g1',
          state: 'completed',
          method: 'code',
          params: { task: 'Write release summary', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          result: [
            { name: 'summary.md', mimeType: 'text/markdown', data: 'Done' },
            { name: 'checks.md', mimeType: 'text/markdown', data: 'Passed' },
          ],
          priority: 'normal',
          progress: 100,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain('2 result files')
    expect(notifications[0].message).not.toContain(['result', 'artifact'].join(' '))
  })

  it('turns missing completed task summaries into a clear next step', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-no-result',
          groupId: 'g1',
          state: 'completed',
          method: 'code',
          params: { task: 'Update onboarding copy', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          priority: 'normal',
          progress: 100,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain(
      'Open the task details to confirm what changed before using the result.'
    )
    expect(notifications[0].message).not.toContain('No completion summary was provided')
  })

  it('hides raw completed task stdout in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-stdout',
          groupId: 'g1',
          state: 'completed',
          method: 'code',
          params: { task: 'Generate report', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          result: {
            stdout: 'panic: stack trace line 7\nsecret token abc\nraw command output',
          },
          priority: 'normal',
          progress: 100,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain(
      'Finished with a text result. Open the task details to check it before using it.'
    )
    expect(notifications[0].message).not.toContain('review it before using it')
    expect(notifications[0].message).not.toContain('panic')
    expect(notifications[0].message).not.toContain('stack trace')
    expect(notifications[0].message).not.toContain('secret token')
    expect(notifications[0].message).not.toContain('raw command output')
  })

  it('hides support-style completed task messages in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-message-raw',
          groupId: 'g1',
          state: 'completed',
          method: 'code',
          params: { task: 'Update deployment notes', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          result: {
            message: 'panic: stack trace line 7\nsecret token abc\nraw command output',
          },
          priority: 'normal',
          progress: 100,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain(
      'Finished with a summary to check. Open the task details before using the result.'
    )
    expect(notifications[0].message).not.toContain('you should check')
    expect(notifications[0].message).not.toContain('panic')
    expect(notifications[0].message).not.toContain('stack trace')
    expect(notifications[0].message).not.toContain('secret token')
    expect(notifications[0].message).not.toContain('raw command output')
  })

  it('hides role-policy completed task messages in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.updated',
        eventId: 'evt-task',
        task: {
          ...WIRE_TASK,
          id: 'task-owner-message-role',
          groupId: 'g1',
          state: 'completed',
          method: 'code',
          params: { task: 'Update team access notes', message: '' },
          createdBy: 'user-owner',
          assignedAgentName: 'Codex',
          result: {
            message: 'owner role required',
          },
          priority: 'normal',
          progress: 100,
          createdAt: '2026-04-03T00:00:00Z',
          updatedAt: '2026-04-03T00:01:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toContain(
      'Finished with a summary to check. Open the task details before using the result.'
    )
    expect(notifications[0].message).not.toContain('owner role required')
  })

  it('notifies the credential owner when a tool account expires', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'credential:status_update',
      payload: {
        action: 'credential.revoked',
        eventId: 'evt-credential-1',
        credential: {
          ownerUserId: 'user-owner',
          cliTool: 'codex',
          status: 'expired',
          reason: 'invalid_grant',
          revokedAt: '2026-04-03T00:02:00Z',
        },
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0]).toMatchObject({
      id: 'credential-owner:user-owner:codex:expired:evt-credential-1',
      type: 'credential_expired',
      taskId: 'credential:codex',
      taskTitle: 'Codex account needs reconnecting',
      ownerUserId: 'user-owner',
      taskHref: '/settings/work-tool-sign-ins',
      read: false,
    })
    expect(notifications[0].message).toContain(
      'Open Codex sign-in, then reconnect the Codex account'
    )
    expect(notifications[0].message).toContain('new agents that work on files')
    expect(notifications[0].message).not.toContain('in Settings before')
    expect(notifications[0].taskTitle).not.toContain('credential expired')
  })

  it('does not notify other users for credential status updates', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'credential:status_update',
      payload: {
        action: 'credential.revoked',
        eventId: 'evt-credential-2',
        credential: {
          ownerUserId: 'someone-else',
          cliTool: 'codex',
          status: 'expired',
          reason: 'invalid_grant',
          revokedAt: '2026-04-03T00:02:00Z',
        },
      },
    })

    expect(useFeedStore.getState().notifications).toHaveLength(0)
  })

  it('toasts admins when a CLI agent tool package is updated and dedups by eventId', () => {
    const frame = {
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'codex',
        state: 'updated',
        localDigest: 'sha256:new',
        remoteDigest: 'sha256:new',
        lastError: null,
        eventId: 'cli-image:codex:updated:sha256:new',
        unix: 1_700_000_000,
      },
    }
    dispatchWsMessage(frame)
    dispatchWsMessage(frame) // redelivery must not double-toast

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0]).toMatchObject({
      id: 'cli-image:codex:updated:sha256:new',
      type: 'cli_image_updated',
      taskId: 'cli-image:codex',
      taskHref: '/admin',
      read: false,
    })
    expect(notifications[0].taskTitle).toContain('agent tool package updated')
    expect(notifications[0].message).toContain('latest tool package')
  })

  it('toasts a failed CLI tool package check with a beginner recovery step', () => {
    dispatchWsMessage({
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'gemini',
        state: 'failed',
        localDigest: null,
        remoteDigest: null,
        lastError: 'registry timeout',
        eventId: 'cli-image:gemini:failed',
        unix: 1_700_000_001,
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].type).toBe('cli_image_updated')
    expect(notifications[0].taskTitle).toContain('Check Gemini tool package in Admin')
    expect(notifications[0].message).toContain(
      'Open Admin and choose Agent tool updates, then choose Check now'
    )
    expect(notifications[0].message).toContain('tool package access')
    expect(notifications[0].message).not.toContain('check failed')
    expect(notifications[0].message).not.toContain('registry timeout')
  })

  it('live-patches an open CLI images panel from the toast', () => {
    useAdminStore.setState({
      cliImages: {
        autoUpdateEnabled: true,
        pollIntervalSecs: 900,
        registry: 'ghcr.io/x',
        imageTag: 'latest',
        tools: [
          {
            tool: 'codex',
            state: 'up_to_date',
            localDigest: 'sha256:old',
            remoteDigest: 'sha256:old',
            lastCheckedUnix: 1,
            lastUpdatedUnix: null,
            lastError: null,
            agentsWithContainer: 1,
          },
        ],
        prune: {
          enabled: false,
          lastRunUnix: null,
          scanned: 0,
          removed: 0,
          skippedInUse: 0,
          skippedConflict: 0,
          errors: 0,
          lastError: null,
        },
      },
    })

    dispatchWsMessage({
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'codex',
        state: 'updated',
        localDigest: 'sha256:new',
        remoteDigest: 'sha256:new',
        lastError: null,
        eventId: 'cli-image:codex:updated:sha256:new',
        unix: 1_700_000_002,
      },
    })

    const codex = useAdminStore.getState().cliImages?.tools.find((t) => t.tool === 'codex')
    expect(codex?.state).toBe('updated')
    expect(codex?.remoteDigest).toBe('sha256:new')
    expect(codex?.lastUpdatedUnix).toBe(1_700_000_002)
    // agentsWithContainer is left untouched (toast carries no count).
    expect(codex?.agentsWithContainer).toBe(1)
  })

  it('a failed cli_image toast patches state but does NOT stamp lastUpdatedUnix', () => {
    useAdminStore.setState({
      cliImages: {
        autoUpdateEnabled: true,
        pollIntervalSecs: 900,
        registry: 'ghcr.io/x',
        imageTag: 'latest',
        tools: [
          {
            tool: 'codex',
            state: 'up_to_date',
            localDigest: 'sha256:old',
            remoteDigest: 'sha256:old',
            lastCheckedUnix: 1,
            lastUpdatedUnix: 42,
            lastError: null,
            agentsWithContainer: 1,
          },
        ],
        prune: {
          enabled: false,
          lastRunUnix: null,
          scanned: 0,
          removed: 0,
          skippedInUse: 0,
          skippedConflict: 0,
          errors: 0,
          lastError: null,
        },
      },
    })

    dispatchWsMessage({
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'codex',
        state: 'failed',
        localDigest: null,
        remoteDigest: null,
        lastError: 'registry timeout',
        eventId: 'cli-image:codex:failed',
        unix: 1_700_000_009,
      },
    })

    const codex = useAdminStore.getState().cliImages?.tools.find((t) => t.tool === 'codex')
    expect(codex?.state).toBe('failed')
    expect(codex?.lastError).toBe('registry timeout')
    expect(codex?.lastCheckedUnix).toBe(1_700_000_009)
    // A failed check did NOT update the image → lastUpdatedUnix must be preserved.
    expect(codex?.lastUpdatedUnix).toBe(42)
  })

  it('a cli_image toast for an unknown tool leaves the report untouched', () => {
    const loaded = {
      autoUpdateEnabled: true,
      pollIntervalSecs: 900,
      registry: 'ghcr.io/x',
      imageTag: 'latest',
      tools: [
        {
          tool: 'codex',
          state: 'up_to_date' as const,
          localDigest: 'sha256:old',
          remoteDigest: 'sha256:old',
          lastCheckedUnix: 1,
          lastUpdatedUnix: null,
          lastError: null,
          agentsWithContainer: 1,
        },
      ],
      prune: {
        enabled: false,
        lastRunUnix: null,
        scanned: 0,
        removed: 0,
        skippedInUse: 0,
        skippedConflict: 0,
        errors: 0,
        lastError: null,
      },
    }
    useAdminStore.setState({ cliImages: loaded })

    dispatchWsMessage({
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'gemini',
        state: 'updated',
        remoteDigest: 'sha256:x',
        eventId: 'e',
        unix: 5,
      },
    })

    // gemini isn't in the report → no row changes; codex is untouched.
    const codex = useAdminStore.getState().cliImages?.tools.find((t) => t.tool === 'codex')
    expect(codex?.state).toBe('up_to_date')
    expect(useAdminStore.getState().cliImages?.tools).toHaveLength(1)
  })

  it('re-surfaces a distinct CLI image failure as unread after the prior one was read', () => {
    // First failure, then the admin reads it.
    dispatchWsMessage({
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'gemini',
        state: 'failed',
        lastError: 'registry timeout',
        eventId: 'cli-image:gemini:failed:aaa',
        unix: 1,
      },
    })
    useFeedStore.getState().markRead('cli-image:gemini:failed:aaa')

    // A genuinely different failure (different producer-side error key) must be a
    // NEW unread notification, not silently merged into the read one.
    dispatchWsMessage({
      type: 'cli_image.updated',
      payload: {
        ...CLI_IMAGE_NULLS,
        tool: 'gemini',
        state: 'failed',
        lastError: 'auth revoked',
        eventId: 'cli-image:gemini:failed:bbb',
        unix: 2,
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(2)
    const fresh = notifications.find((n) => n.id === 'cli-image:gemini:failed:bbb')
    expect(fresh?.read).toBe(false)
    expect(fresh?.message).toContain('Open Admin and choose Agent tool updates')
    expect(fresh?.message).not.toContain('check failed')
    expect(fresh?.message).not.toContain('auth revoked')
  })

  it('ignores a cli_image toast for a panel that has not loaded', () => {
    expect(() =>
      dispatchWsMessage({
        type: 'cli_image.updated',
        payload: { ...CLI_IMAGE_NULLS, tool: 'codex', state: 'updated', eventId: 'x', unix: 1 },
      })
    ).not.toThrow()
    expect(useAdminStore.getState().cliImages).toBeNull()
    // still toasts even when the panel was never opened
    expect(useFeedStore.getState().notifications).toHaveLength(1)
  })
})

// ---------------------------------------------------------------------------
// Golden fixtures (#1327): every file in tests/fixtures/ws-protocol/ goes through
// the real dispatcher with real stores. WS_PROTOCOL_FIXTURE_DIR points the suite
// at a copy of the directory (used to prove the suite fails on a bad fixture).
// ---------------------------------------------------------------------------

const FIXTURE_DIR =
  process.env.WS_PROTOCOL_FIXTURE_DIR ?? join(process.cwd(), 'tests/fixtures/ws-protocol')
const fixtureFiles = readdirSync(FIXTURE_DIR)
  .filter((file) => file.endsWith('.json'))
  .sort()

function readFixture(file: string): unknown {
  return JSON.parse(readFileSync(join(FIXTURE_DIR, file), 'utf8'))
}

function fixtureType(fixture: unknown): string | undefined {
  if (fixture === null || typeof fixture !== 'object' || !('type' in fixture)) return undefined
  return typeof fixture.type === 'string' ? fixture.type : undefined
}

function registeredFixture(type: string): string | undefined {
  const entry = Object.entries(WS_FRAME_FIXTURES).find(([key]) => key === type)?.[1]
  return entry && 'fixture' in entry ? entry.fixture : undefined
}

function isIgnoredType(type: string): boolean {
  return Object.keys(IGNORED_WS_FRAMES).includes(type)
}

// The observable effect each HANDLED fixture must have, with the state it needs.
// Values are copied from the fixture files.
const FIXTURE_EFFECTS: Record<string, { seed?: () => void; check: () => void }> = {
  event: {
    check: () => {
      expect(useFeedStore.getState().feedItems).toEqual([
        expect.objectContaining({
          type: 'pre_tool_use',
          taskTitle: 'Updating project files',
          agentName: 'The agent',
          timestamp: 1735732800000,
        }),
      ])
    },
  },
  'cli_image.updated': {
    check: () => {
      expect(useFeedStore.getState().notifications).toEqual([
        expect.objectContaining({
          id: 'cli-image:codex:updated:sha256:00000000000000000000000000000000000000000000000000000000000000ab',
          type: 'cli_image_updated',
          taskTitle: 'Codex agent tool package updated',
          timestamp: 1735732800000,
        }),
      ])
    },
  },
  'orchestration:participant_update': {
    check: () => {
      expect(useFeedStore.getState().agents).toEqual([
        { id: '00000000-0000-0000-0000-000000000014', name: 'agent-00000000', status: 'idle' },
      ])
    },
  },
  'orchestration:task_update': {
    seed: () => {
      useBoardStore.getState().setSelectedGroupId('00000000-0000-0000-0000-000000000022')
      localStorage.setItem(
        'af:auth:user',
        JSON.stringify({ id: '00000000-0000-0000-0000-000000000023' })
      )
    },
    check: () => {
      expect(useBoardStore.getState().columns.blocked).toEqual([
        expect.objectContaining({
          id: '00000000-0000-0000-0000-000000000021',
          state: 'blocked',
          selfFix: true,
          contextCounts: { appliedMemories: 3, appliedSkills: 5, total: 8 },
        }),
      ])
      expect(useFeedStore.getState().notifications).toEqual([
        expect.objectContaining({
          id: 'task-owner:00000000-0000-0000-0000-000000000021:blocked',
          taskTitle: 'Deploy staging',
          ownerUserId: '00000000-0000-0000-0000-000000000023',
        }),
      ])
    },
  },
  'project_clone:status_update': {
    seed: () => {
      useNavigationStore.setState({
        projects: {
          'team-1': [
            {
              id: '00000000-0000-0000-0000-000000000003',
              teamId: 'team-1',
              name: 'Web App',
              slug: 'web-app',
              color: '#0066cc',
              description: '',
            },
          ],
        },
      })
    },
    check: () => {
      expect(useNavigationStore.getState().projects['team-1']?.[0]).toMatchObject({
        cloneStatus: 'ready',
        clone: {
          status: 'ready',
          attempt: 1,
          resolvedBranch: 'main',
          headSha: '0000000000000000000000000000000000000abc',
        },
      })
    },
  },
}

describe('golden ws-protocol fixtures', () => {
  beforeEach(() => {
    useNavigationStore.getState().reset()
  })

  it('registers every fixture file, and every registered fixture exists', () => {
    expect(fixtureFiles.length).toBeGreaterThan(0)
    for (const file of fixtureFiles) {
      const type = fixtureType(readFixture(file))
      if (type === undefined) {
        expect(CONTROL_FRAME_FIXTURES, `${file} has no type`).toContain(file)
      } else {
        expect(registeredFixture(type), `${file} (type ${type})`).toBe(file)
      }
    }
    for (const entry of Object.values(WS_FRAME_FIXTURES)) {
      if ('fixture' in entry) expect(fixtureFiles).toContain(entry.fixture)
    }
    for (const file of CONTROL_FRAME_FIXTURES) expect(fixtureFiles).toContain(file)
  })

  it.each(fixtureFiles)('dispatches %s with its expected effect', (file) => {
    const fixture = readFixture(file)
    const type = fixtureType(fixture)
    const effect = type === undefined ? undefined : FIXTURE_EFFECTS[type]
    effect?.seed?.()

    const outcome = dispatchWsMessage(fixture)

    if (type === undefined || isIgnoredType(type)) {
      // Control frames and ignored types must not touch any store.
      expect(outcome).toBe('ignored')
      expect(useFeedStore.getState().feedItems).toHaveLength(0)
      expect(useFeedStore.getState().notifications).toHaveLength(0)
      return
    }
    expect(outcome).toBe('handled')
    expect(effect, `no FIXTURE_EFFECTS entry for ${type}`).toBeDefined()
    effect?.check()
  })
})
