import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { dispatchWsMessage } from '@app/hooks/useWsDispatch'
import { handleOrchestrationWsMessage } from '@app/features/orchestration/model/orchestrationRealtime'
import { useFeedStore } from '@app/shared/model/feed.store'
import { useWorkflowStore } from '@app/features/orchestration/model/workflowStore'

beforeEach(() => {
  useFeedStore.getState().reset()
  useWorkflowStore.getState().reset()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('handleOrchestrationWsMessage', () => {
  it('workflow:status adds a feed item describing the workflow outcome', () => {
    handleOrchestrationWsMessage({
      type: 'workflow:status',
      payload: { workflowId: 'wf-12345678abc', status: 'completed' },
    })

    const items = useFeedStore.getState().feedItems
    expect(items).toHaveLength(1)
    expect(items[0]?.type).toBe('workflow_status')
    expect(items[0]?.taskTitle).toContain('wf-12345')
    expect(items[0]?.taskTitle).toContain('completed')
  })

  it('review.escalated surfaces a notification on the existing inbox path', () => {
    handleOrchestrationWsMessage({
      type: 'review.escalated',
      payload: {
        reviewId: 'review-1',
        taskId: 'task-1',
        dueAt: '2026-06-22T00:00:00.000Z',
        overdueSecs: 7200,
      },
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    const notification = notifications[0]
    expect(notification?.type).toBe('review_escalated')
    expect(notification?.id).toBe('review-escalated:review-1')
    expect(notification?.taskId).toBe('task-1')
    expect(notification?.taskHref).toBe('/tasks')
    expect(notification?.read).toBe(false)
    // The overdue window is rendered in beginner-friendly language.
    expect(notification?.message).toContain('2 hours')
  })

  it('review.escalated dedups a redelivered escalation by review id', () => {
    const frame = {
      type: 'review.escalated' as const,
      payload: { reviewId: 'review-1', taskId: 'task-1', overdueSecs: 60 },
    }
    handleOrchestrationWsMessage(frame)
    handleOrchestrationWsMessage(frame)
    expect(useFeedStore.getState().notifications).toHaveLength(1)
  })

  it('workflow:node_status updates the workflow store keyed by workflow + node', () => {
    handleOrchestrationWsMessage({
      type: 'workflow:node_status',
      payload: {
        workflowId: 'wf-1',
        nodeId: 'node-a',
        nodeName: 'Build',
        status: 'running',
        detail: 'compiling',
      },
    })

    const node = useWorkflowStore.getState().workflows['wf-1']?.['node-a']
    expect(node).toEqual({ name: 'Build', status: 'running', detail: 'compiling' })
  })

  it('workflow:node_status without a workflowId falls back to an "unknown" group', () => {
    handleOrchestrationWsMessage({
      type: 'workflow:node_status',
      payload: { nodeId: 'node-x', status: 'completed' },
    })

    const node = useWorkflowStore.getState().workflows.unknown?.['node-x']
    expect(node?.status).toBe('completed')
    // nodeName falls back to the nodeId when absent.
    expect(node?.name).toBe('node-x')
  })

  it('ignores a malformed workflow:status frame without throwing', () => {
    expect(() =>
      handleOrchestrationWsMessage({ type: 'workflow:status', payload: { status: 'completed' } })
    ).not.toThrow()
    expect(useFeedStore.getState().feedItems).toHaveLength(0)
  })

  it('ignores a malformed review.escalated frame without throwing', () => {
    expect(() =>
      handleOrchestrationWsMessage({ type: 'review.escalated', payload: { taskId: 'task-1' } })
    ).not.toThrow()
    expect(useFeedStore.getState().notifications).toHaveLength(0)
  })

  it('logs a malformed review.escalated unconditionally so prod leaves a breadcrumb', () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    // dev:false forces the production logging path. review.escalated is the
    // highest-value alert, so a missing reviewId must still warn (not be DEV-only).
    handleOrchestrationWsMessage({ type: 'review.escalated', payload: { taskId: 'task-1' } }, { dev: false })

    expect(useFeedStore.getState().notifications).toHaveLength(0)
    expect(warnSpy).toHaveBeenCalledTimes(1)
    expect(warnSpy.mock.calls[0]?.[0]).toContain('review.escalated missing reviewId')
  })

  it('does not log a malformed high-frequency node_status in production', () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    // node_status is high-frequency and non-critical: its breadcrumb stays
    // DEV-only to avoid console spam in production.
    handleOrchestrationWsMessage({ type: 'workflow:node_status', payload: { status: 'running' } }, { dev: false })

    expect(errorSpy).not.toHaveBeenCalled()
    expect(warnSpy).not.toHaveBeenCalled()
  })
})

describe('dispatchWsMessage routes orchestration events to the handler', () => {
  it('routes workflow:status through the central dispatcher', () => {
    dispatchWsMessage({
      type: 'workflow:status',
      orgId: 'org-1',
      payload: { workflowId: 'wf-1', status: 'failed' },
    })
    const items = useFeedStore.getState().feedItems
    expect(items).toHaveLength(1)
    expect(items[0]?.taskTitle).toContain('failed')
  })

  it('routes review.escalated through the central dispatcher', () => {
    dispatchWsMessage({
      type: 'review.escalated',
      orgId: 'org-1',
      payload: { reviewId: 'review-9', taskId: 'task-9', overdueSecs: 90000 },
    })
    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0]?.id).toBe('review-escalated:review-9')
    // ~25h overdue rolls up to a day.
    expect(notifications[0]?.message).toContain('1 day')
  })

  it('routes workflow:node_status through the central dispatcher', () => {
    dispatchWsMessage({
      type: 'workflow:node_status',
      orgId: 'org-1',
      payload: { workflowId: 'wf-2', nodeId: 'n1', nodeName: 'Test', status: 'running' },
    })
    expect(useWorkflowStore.getState().workflows['wf-2']?.n1?.status).toBe('running')
  })
})
