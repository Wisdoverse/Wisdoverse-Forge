import { describe, test, expect, afterEach, beforeEach } from 'vitest'
import { render, screen, cleanup, fireEvent, within } from '@testing-library/react'
import { ActivityFeed } from '@app/features/feed/ActivityFeed'
import { useFeedStore } from '@app/shared/model/feed.store'

afterEach(cleanup)
beforeEach(() => useFeedStore.getState().reset())

describe('ActivityFeed', () => {
  test('renders agent status bar', () => {
    useFeedStore.getState().setAgents([{ id: 'a1', name: 'Agent One', status: 'working' }])
    render(<ActivityFeed />)
    expect(screen.getByTestId('agent-status-bar')).toBeDefined()
    expect(screen.getByText('Agent One')).toBeDefined()
  })

  test('renders attention zone when blocked tasks exist', () => {
    useFeedStore.getState().addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy staging',
      agentName: 'Agent Two',
      reason: 'Needs SSH key',
      timestamp: Date.now(),
    })
    render(<ActivityFeed />)
    expect(screen.getByTestId('attention-zone')).toBeDefined()
    expect(screen.getByText('Deploy staging')).toBeDefined()
    expect(screen.getByText('Needs your decision')).toBeDefined()
    expect(screen.getByText(/open each request before choosing what happens next/i)).toBeDefined()
    expect(screen.getByText(/Agent Two is waiting: Waiting for account access/i)).toBeDefined()
    expect(screen.queryByText(/Needs SSH key/i)).toBeNull()
    expect(screen.getByRole('button', { name: /review request/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /approve request/i })).toBeDefined()
  })

  test('hides attention zone when no blocked tasks', () => {
    render(<ActivityFeed />)
    expect(screen.queryByTestId('attention-zone')).toBeNull()
  })

  test('renders feed items', () => {
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.completed',
      agentName: 'Agent One',
      taskTitle: 'Fix auth',
      detail: '2 files changed',
      timestamp: Date.now(),
    })
    render(<ActivityFeed />)
    expect(screen.getByText('Fix auth')).toBeDefined()
  })

  test('summarizes current work with beginner guidance', () => {
    useFeedStore.getState().setAgents([
      { id: 'a1', name: 'Agent One', status: 'working' },
      { id: 'a2', name: 'Agent Two', status: 'blocked' },
    ])
    useFeedStore.getState().addAttentionItem({
      id: 't1',
      taskTitle: 'Deploy staging',
      agentName: 'Agent Two',
      reason: 'Needs SSH key',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.failed',
      agentName: 'Agent Two',
      taskTitle: 'Deploy staging',
      detail: 'SSH key rejected',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '2',
      type: 'task.completed',
      agentName: 'Agent One',
      taskTitle: 'Fix auth',
      detail: '2 files changed',
      timestamp: Date.now(),
    })

    render(<ActivityFeed />)

    const summary = screen.getByTestId('feed-ops-summary')
    expect(within(summary).getByText('Current work')).toBeDefined()
    expect(within(summary).getByText(/start with anything that needs action/i)).toBeDefined()
    expect(screen.getByTestId('feed-review-guide')).toBeDefined()
    expect(screen.getByText('Check order')).toBeDefined()
    expect(screen.getByText(/handle needs action first/i)).toBeDefined()
    expect(screen.getByText(/Waiting for account access/i)).toBeDefined()
    expect(screen.queryByText(/Needs SSH key/i)).toBeNull()
    expect(screen.queryByText(/SSH key rejected/i)).toBeNull()
    expect(within(screen.getByTestId('feed-metric-working')).getByText('Working')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-working')).getByText('1')).toBeDefined()
    expect(
      within(screen.getByTestId('feed-metric-needs-action')).getByText('Needs action')
    ).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-needs-action')).getByText('3')).toBeDefined()
    expect(
      within(screen.getByTestId('feed-metric-updates')).getByText('Recent updates')
    ).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-updates')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('feed-metric-completed')).getByText('Completed')).toBeDefined()
  })

  test('filters live feed by update category', () => {
    useFeedStore.getState().addFeedItem({
      id: '1',
      type: 'task.failed',
      agentName: 'Agent Two',
      taskTitle: 'Deploy staging',
      detail: 'SSH key rejected',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '2',
      type: 'task.progress',
      agentName: 'Agent One',
      taskTitle: 'Fix auth',
      detail: 'Editing tests',
      timestamp: Date.now(),
    })
    useFeedStore.getState().addFeedItem({
      id: '3',
      type: 'task.completed',
      agentName: 'Agent One',
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
      agentName: 'Agent One',
      taskTitle: 'Fix auth',
      detail: 'Editing tests',
      timestamp: Date.now(),
    })

    render(<ActivityFeed />)

    fireEvent.click(screen.getByRole('button', { name: /completed\s*0/i }))
    const emptyState = screen.getByTestId('feed-filter-empty')
    expect(within(emptyState).getByText('Completed updates will appear here')).toBeDefined()
    expect(within(emptyState).getByText(/finished work shows here/i)).toBeDefined()
    expect(within(emptyState).getByText(/see what happened most recently/i)).toBeDefined()
    expect(emptyState.textContent).not.toContain('No completed updates in this view')
    expect(screen.queryByText('Fix auth')).toBeNull()

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all updates/i }))

    expect(screen.getByText('Fix auth')).toBeDefined()
  })

  test('shows empty state when no feed items', () => {
    render(<ActivityFeed />)
    expect(screen.getByText(/quiet so far/i)).toBeDefined()
    expect(screen.getByText(/start a task or wait for the assigned agent/i)).toBeDefined()
    expect(screen.getByText(/open Board, create or assign a task/i)).toBeDefined()
    expect(screen.queryByText(/No progress updates yet/i)).toBeNull()
    expect(screen.queryByText(/No work has reported progress yet/i)).toBeNull()
  })
})
