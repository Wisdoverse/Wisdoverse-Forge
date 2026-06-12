import { describe, it, expect, beforeEach } from 'vitest'
import { useAdminStore } from '@app/shared/model/admin.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useContextStore } from '@app/shared/model/context.store'
import { useFeedStore } from '@app/shared/model/feed.store'
import { dispatchWsMessage } from '@app/hooks/useWsDispatch'
import { resetContextHandlerDedupeForTests } from '@app/features/context/model/contextRealtime'

beforeEach(() => {
  useBoardStore.getState().reset()
  useContextStore.getState().reset()
  useFeedStore.getState().reset()
  useAdminStore.setState({ cliImages: null, cliImagesLoading: false, cliImagesError: null })
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
    expect(useFeedStore.getState().feedItems[0].taskTitle).toBe('Checking project files')
    expect(useFeedStore.getState().feedItems[0].detail).toBe('Started checking project files.')
    expect(useFeedStore.getState().feedItems[0].detail).not.toContain('Tool:')
    expect(useFeedStore.getState().feedItems[0].taskTitle).not.toBe('Read')
  })

  it('turns command activity events into plain work steps', () => {
    dispatchWsMessage({
      type: 'event',
      payload: {
        type: 'post_tool_use',
        agentName: 'Codex',
        tool: 'Bash',
        timestamp: Date.now(),
      },
    })

    const item = useFeedStore.getState().feedItems[0]
    expect(item.taskTitle).toBe('Finished running a project command')
    expect(item.detail).toBe('Finished running a project command.')
    expect(item.detail).not.toContain('Tool:')
    expect(item.taskTitle).not.toBe('Bash')
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

  it('hides raw blocked fallback details in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        action: 'updated',
        task: {
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
        task: {
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
    expect(notifications[0].message).toContain(
      'Stopped because sign-in or service access needs attention.'
    )
    expect(notifications[0].message).toContain(
      'stopped before finishing. Open the task, review the recovery note, then retry or choose another agent.'
    )
    expect(notifications[0].message).not.toContain('when ready')
    expect(notifications[0].message).not.toContain('reassign')
    expect(notifications[0].message).not.toContain('failed to complete this task')
    expect(notifications[0].message).not.toContain('401 Unauthorized')
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
    expect(notifications[0].message).toContain(
      'Stopped before finishing. Open details to see what happened and retry.'
    )
    expect(notifications[0].message).not.toContain('migration exited non-zero')
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

  it('describes completed result arrays as result files in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        task: {
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

  it('hides raw completed task stdout in owner notifications', () => {
    localStorage.setItem('af:auth:user', JSON.stringify({ id: 'user-owner' }))

    dispatchWsMessage({
      type: 'orchestration:task_update',
      payload: {
        task: {
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
      'Finished with a text result. Open details to review it.'
    )
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
        task: {
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
      'Finished with a summary that needs review. Open details before using the result.'
    )
    expect(notifications[0].message).not.toContain('panic')
    expect(notifications[0].message).not.toContain('stack trace')
    expect(notifications[0].message).not.toContain('secret token')
    expect(notifications[0].message).not.toContain('raw command output')
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
      taskHref: '/settings',
      read: false,
    })
    expect(notifications[0].message).toContain('Reconnect the Codex account in Settings')
    expect(notifications[0].message).toContain('managed workspace agents')
    expect(notifications[0].taskTitle).not.toContain('credential expired')
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

  it('toasts admins when a CLI agent tool package is updated and dedups by eventId', () => {
    const frame = {
      type: 'cli_image.updated',
      payload: {
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
    expect(notifications[0].taskTitle).toContain('tool package check failed')
    expect(notifications[0].message).toContain('Open Admin and choose Check now')
    expect(notifications[0].message).toContain('tool package access')
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
    expect(fresh?.message).toContain('Open Admin and choose Check now')
    expect(fresh?.message).not.toContain('auth revoked')
  })

  it('ignores a cli_image toast for a panel that has not loaded', () => {
    expect(() =>
      dispatchWsMessage({
        type: 'cli_image.updated',
        payload: { tool: 'codex', state: 'updated', eventId: 'x', unix: 1 },
      })
    ).not.toThrow()
    expect(useAdminStore.getState().cliImages).toBeNull()
    // still toasts even when the panel was never opened
    expect(useFeedStore.getState().notifications).toHaveLength(1)
  })
})
