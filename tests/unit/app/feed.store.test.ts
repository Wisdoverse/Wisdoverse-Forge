import { describe, test, expect, beforeEach } from 'vitest'
import { useFeedStore } from '@app/shared/model/feed.store'

beforeEach(() => useFeedStore.getState().reset())

describe('Feed Store', () => {
  test('initializes with empty feed and agents', () => {
    const state = useFeedStore.getState()
    expect(state.feedItems).toEqual([])
    expect(state.agents).toEqual([])
    expect(state.attentionItems).toEqual([])
  })

  test('addFeedItem prepends to feed', () => {
    const store = useFeedStore.getState()
    store.addFeedItem({
      id: '1',
      type: 'task.completed',
      agentName: 'Claude-1',
      taskTitle: 'Fix bug',
      detail: '2 files changed',
      timestamp: Date.now(),
    })
    store.addFeedItem({
      id: '2',
      type: 'task.queued',
      agentName: 'Claude-2',
      taskTitle: 'Add tests',
      detail: '',
      timestamp: Date.now(),
    })
    const { feedItems } = useFeedStore.getState()
    expect(feedItems).toHaveLength(2)
    expect(feedItems[0].id).toBe('2')
  })

  test('setAgents updates agent list', () => {
    useFeedStore.getState().setAgents([
      { id: 'a1', name: 'Claude-1', status: 'working' },
      { id: 'a2', name: 'Gemini-1', status: 'idle' },
    ])
    expect(useFeedStore.getState().agents).toHaveLength(2)
  })

  test('addAttentionItem adds blocked task', () => {
    useFeedStore.getState().addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy staging',
      agentName: 'GPT-1',
      reason: 'Needs SSH key approval',
      timestamp: Date.now(),
    })
    expect(useFeedStore.getState().attentionItems).toHaveLength(1)
  })

  test('addAttentionItem stores beginner-safe reasons', () => {
    useFeedStore.getState().addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy staging',
      agentName: 'GPT-1',
      reason: 'Needs SSH key approval',
      timestamp: Date.now(),
    })

    const [item] = useFeedStore.getState().attentionItems
    expect(item.reason).toContain('Waiting for account access')
    expect(item.reason).not.toContain('SSH key')
  })

  test('removeAttentionItem removes by id', () => {
    const store = useFeedStore.getState()
    store.addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy',
      agentName: 'GPT-1',
      reason: 'Needs approval',
      timestamp: Date.now(),
    })
    store.removeAttentionItem('t1')
    expect(useFeedStore.getState().attentionItems).toHaveLength(0)
  })

  test('feed is capped at 100 items', () => {
    const store = useFeedStore.getState()
    for (let i = 0; i < 110; i++) {
      store.addFeedItem({
        id: `${i}`,
        type: 'task.progress',
        agentName: 'C1',
        taskTitle: `Task ${i}`,
        detail: '',
        timestamp: Date.now(),
      })
    }
    expect(useFeedStore.getState().feedItems).toHaveLength(100)
  })

  test('addNotification is idempotent by notification id', () => {
    const store = useFeedStore.getState()
    store.addNotification({
      id: 'task-owner:t1:blocked',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Deploy',
      message: 'Blocked',
      read: false,
      timestamp: 1,
    })
    store.markRead('task-owner:t1:blocked')
    store.addNotification({
      id: 'task-owner:t1:blocked',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Deploy',
      message: 'Still blocked',
      read: false,
      timestamp: 2,
    })

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(1)
    expect(notifications[0].message).toBe('Still blocked')
    expect(notifications[0].read).toBe(true)
  })

  test('caps retained notifications so distinct-id producers cannot grow unbounded', () => {
    const store = useFeedStore.getState()
    // Each distinct id is a separate notification (mirrors per-version CLI tool package toasts).
    for (let i = 0; i < 150; i += 1) {
      store.addNotification({
        id: `cli-image:codex:updated:sha256:${i}`,
        type: 'cli_image_updated',
        taskId: 'cli-image:codex',
        taskTitle: 'codex agent tool package updated',
        message: `version ${i}`,
        read: false,
        timestamp: i,
      })
    }

    const notifications = useFeedStore.getState().notifications
    expect(notifications).toHaveLength(100)
    // Newest is retained at the head; oldest are evicted.
    expect(notifications[0].id).toBe('cli-image:codex:updated:sha256:149')
  })
})
