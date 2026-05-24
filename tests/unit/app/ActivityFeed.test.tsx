import { describe, test, expect, afterEach, beforeEach } from 'vitest'
import { render, screen, cleanup, fireEvent, within } from '@testing-library/react'
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

  test('summarizes managed agent operations', () => {
    useFeedStore.getState().setAgents([
      { id: 'a1', name: 'Claude-1', status: 'working' },
      { id: 'a2', name: 'Codex Host', status: 'blocked' },
    ])
    useFeedStore.getState().addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy staging',
      agentName: 'Codex Host',
      reason: 'Needs SSH key',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.failed',
      agentName: 'Codex Host',
      taskTitle: 'Deploy staging',
      detail: 'SSH key rejected',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '2',
      type: 'task.completed',
      agentName: 'Claude-1',
      taskTitle: 'Fix auth',
      detail: '2 files changed',
      timestamp: Date.now(),
    })

    render(<ActivityFeed />)

    const summary = screen.getByTestId('feed-ops-summary')
    expect(within(summary).getByText('Agent operations')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-working')).getByText('Working')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-working')).getByText('1')).toBeDefined()
    expect(
      within(screen.getByTestId('feed-metric-needs-action')).getByText('Needs action')
    ).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-needs-action')).getByText('3')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-updates')).getByText('Updates')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-updates')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-completed')).getByText('Completed')).toBeDefined()
  })

  test('filters live feed by update category', () => {
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.failed',
      agentName: 'Codex Host',
      taskTitle: 'Deploy staging',
      detail: 'SSH key rejected',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '2',
      type: 'task.progress',
      agentName: 'Claude-1',
      taskTitle: 'Fix auth',
      detail: 'Editing tests',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '3',
      type: 'task.completed',
      agentName: 'Claude-1',
      taskTitle: 'Ship patch',
      detail: 'Merged cleanly',
      timestamp: Date.now(),
    })

    render(<ActivityFeed />)

    const filters = screen.getByTestId('feed-filter-group')
    fireEvent.click(within(filters).getByRole('button', { name: /needs action\s*1/i }))

    expect(screen.getByText('Deploy staging')).toBeDefined()
    expect(screen.queryByText('Fix auth')).toBeNull()
    expect(screen.queryByText('Ship patch')).toBeNull()

    fireEvent.click(within(filters).getByRole('button', { name: /progress\s*1/i }))

    expect(screen.getByText('Fix auth')).toBeDefined()
    expect(screen.queryByText('Deploy staging')).toBeNull()
  })

  test('shows filtered empty state when a feed category has no items', () => {
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.progress',
      agentName: 'Claude-1',
      taskTitle: 'Fix auth',
      detail: 'Editing tests',
      timestamp: Date.now(),
    })

    render(<ActivityFeed />)

    fireEvent.click(screen.getByRole('button', { name: /completed\s*0/i }))
    expect(screen.getByText(/no updates in this view/i)).toBeDefined()
    expect(screen.queryByText('Fix auth')).toBeNull()
  })

  test('shows empty state when no feed items', () => {
    render(<ActivityFeed />)
    expect(screen.getByText(/quiet so far/i)).toBeDefined()
  })
})
