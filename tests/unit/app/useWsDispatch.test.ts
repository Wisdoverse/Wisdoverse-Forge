import { describe, it, expect, beforeEach } from 'vitest'
import { useBoardStore } from '@app/shared/model/board.store'
import { useContextStore } from '@app/shared/model/context.store'
import { useFeedStore } from '@app/shared/model/feed.store'
import { dispatchWsMessage } from '@app/hooks/useWsDispatch'
import { resetContextHandlerDedupeForTests } from '@app/features/context/model/contextRealtime'

beforeEach(() => {
  useBoardStore.getState().reset()
  useContextStore.getState().reset()
  useFeedStore.getState().reset()
  resetContextHandlerDedupeForTests()
  localStorage.clear()
})

describe('dispatchWsMessage', () => {
  it('dispatches orchestration:task_update to board store when group matches', () => {
    useBoardStore.getState().setSelectedGroupId('g1')

    const task = {
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
      payload: { action: 'updated', task, groupId: 'g1' },
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
        task: {
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

  it('dispatches agent_update to feed store', () => {
    dispatchWsMessage({
      type: 'agent_update',
      payload: { id: 'a1', name: 'Claude', status: 'working' },
    })

    const agents = useFeedStore.getState().agents
    expect(agents).toHaveLength(1)
    expect(agents[0].name).toBe('Claude')
  })

  it('dispatches agents to feed store', () => {
    dispatchWsMessage({
      type: 'agents',
      payload: [
        { id: 'a1', name: 'Claude', status: 'idle' },
        { id: 'a2', name: 'Codex', status: 'working' },
      ],
    })

    expect(useFeedStore.getState().agents).toHaveLength(2)
  })

  it('dispatches event to feed items', () => {
    dispatchWsMessage({
      type: 'event',
      payload: {
        type: 'pre_tool_use',
        agentName: 'Claude',
        tool: 'Read',
        timestamp: Date.now(),
      },
    })

    expect(useFeedStore.getState().feedItems).toHaveLength(1)
    expect(useFeedStore.getState().feedItems[0].type).toBe('pre_tool_use')
  })

  it('ignores unknown message types', () => {
    expect(() => dispatchWsMessage({ type: 'pong' })).not.toThrow()
    expect(() => dispatchWsMessage({ type: 'unknown_thing' })).not.toThrow()
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
    dispatchWsMessage({ type: 'context_candidate.created', candidateId: 'candidate-ws-1' })
    dispatchWsMessage({ type: 'context_candidate.rejected', candidateId: 'candidate-ws-1' })

    expect(useContextStore.getState().pendingCandidateCount).toBe(0)
  })

  it('notifies the human task owner when their task becomes blocked', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        task: {
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
    expect(notifications[0].message).toContain('Waiting for SSH approval')
    expect(notifications[0].message).toContain('needs owner input')
  })

  it('notifies the human task owner when their task fails', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        task: {
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
    expect(notifications[0].message).toContain('failed')
    expect(notifications[0].message).toContain('401 Unauthorized')
    expect(notifications[0].message).toContain('failed to complete this task')
  })

  it('notifies for legacy error task updates with snake_case owner fields', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'task.failed',
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

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0]).toMatchObject({
      id: 'task-owner:task-owner-error:failed',
      type: 'failed',
      taskId: 'task-owner-error',
      ownerUserId: 'user-owner',
    })
    expect(notifications[0].message).toContain('migration exited non-zero')
  })

  it('does not notify non-owners and does not duplicate repeated completed events', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    const task = {
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

    dispatchWsMessage({ type: 'orchestration:task_update', payload: { task } })
    dispatchWsMessage({ type: 'orchestration:task_update', payload: { task } })
    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: { task: { ...task, id: 'task-owner-3', createdBy: 'someone-else' } },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].id).toBe('task-owner:task-owner-2:completed')
    expect(notifications[0].message).toContain('Patch merged')
  })

  it('notifies the credential owner when a Container CLI credential expires', () => {
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
      taskTitle: 'Codex credential expired',
      ownerUserId: 'user-owner',
      taskHref: '/settings',
      read: false,
    })
    expect(notifications[0].message).toContain('Reconnect Codex auth in Settings')
  })

  it('does not notify other users for credential status updates', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'credential:status_update',
      payload: {
        action: 'credential.revoked',
        credential: {
          ownerUserId: 'someone-else',
          cliTool: 'codex',
          status: 'expired',
        },
      },
    })

    expect(useFeedStore.getState().notifications).toHaveLength(0)
  })
})
