import { describe, test, expect, afterEach, beforeEach } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { ActivityFeed } from '@app/features/feed/ActivityFeed'
import { useFeedStore } from '@app/shared/model/feed.store'

afterEach(cleanup)
beforeEach(() => useFeedStore.getState().reset())

describe('ActivityFeed', () => {
  test('renders agent status bar', () => {
    useFeedStore.getState().setAgents([{ id: 'a1', name: 'Claude-1', status: 'working' }])
    render(<ActivityFeed />)
    expect(screen.getByTestId('agent-status-bar')).toBeDefined()
    expect(screen.getByText('Claude-1')).toBeDefined()
  })

  test('renders attention zone when blocked tasks exist', () => {
    useFeedStore.getState().addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy staging',
      agentName: 'GPT-1',
      reason: 'Needs SSH key',
      timestamp: Date.now(),
    })
    render(<ActivityFeed />)
    expect(screen.getByTestId('attention-zone')).toBeDefined()
    expect(screen.getByText('Deploy staging')).toBeDefined()
  })

  test('hides attention zone when no blocked tasks', () => {
    render(<ActivityFeed />)
    expect(screen.queryByTestId('attention-zone')).toBeNull()
  })

  test('renders feed items', () => {
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.completed',
      agentName: 'Claude-1',
      taskTitle: 'Fix auth',
      detail: '2 files changed',
      timestamp: Date.now(),
    })
    render(<ActivityFeed />)
    expect(screen.getByText('Fix auth')).toBeDefined()
  })

  test('shows empty state when no feed items', () => {
    render(<ActivityFeed />)
    expect(screen.getByText(/quiet so far/i)).toBeDefined()
  })
})
