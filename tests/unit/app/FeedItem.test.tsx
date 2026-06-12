import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { FeedItem } from '@app/features/feed/FeedItem'
import type { FeedItem as FeedItemType } from '@app/shared/model/feed.store'

const baseItem: FeedItemType = {
  id: 'feed-1',
  type: 'task.progress',
  agentName: 'Builder',
  taskTitle: 'Update checkout flow',
  detail: 'Editing tests',
  timestamp: new Date('2026-05-25T12:00:00.000Z').getTime(),
}

afterEach(cleanup)

describe('FeedItem', () => {
  test('uses outcome labels that explain task updates to non-specialists', () => {
    render(
      <FeedItem item={{ ...baseItem, type: 'task.blocked', detail: 'Needs repository key' }} />
    )

    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.queryByText('Blocked')).toBeNull()
    expect(screen.getByText(/Waiting for account access/i)).toBeDefined()
    expect(screen.queryByText(/repository key/i)).toBeNull()
    expect(
      screen.getByLabelText(
        /needs help: builder on update checkout flow\. the task is waiting for someone to clear a blocker/i
      )
    ).toBeDefined()
    expect(screen.getByText(/next step: open the task and clear the blocker/i)).toBeDefined()
  })

  test('shows a retry-safe next step for failed task updates', () => {
    render(<FeedItem item={{ ...baseItem, type: 'task.failed', detail: 'Command exited 1' }} />)

    expect(screen.getByText('Needs review')).toBeDefined()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(
      screen.getByText('Open details to see the recovery note, then retry or reassign when ready.')
    ).toBeDefined()
    expect(screen.getByText(/follow the recovery note, then retry when ready/i)).toBeDefined()
    expect(screen.queryByText('Command exited 1')).toBeNull()
    expect(screen.queryByText(/read the error/i)).toBeNull()
  })

  test('hides sensitive failed task details', () => {
    render(<FeedItem item={{ ...baseItem, type: 'task.failed', detail: 'SSH key rejected' }} />)

    expect(
      screen.getByText('Open details to see the recovery note, then retry or reassign when ready.')
    ).toBeDefined()
    expect(screen.queryByText(/SSH key rejected/i)).toBeNull()
  })

  test('keeps readable failed task details when they are already safe', () => {
    render(
      <FeedItem
        item={{ ...baseItem, type: 'task.failed', detail: 'Repository access needs reconnecting' }}
      />
    )

    expect(screen.getByText('Repository access needs reconnecting')).toBeDefined()
    expect(screen.queryByText(/HTTP 500/i)).toBeNull()
  })

  test('shows waiting and finished labels instead of raw queue status words', () => {
    const { rerender } = render(<FeedItem item={{ ...baseItem, type: 'task.queued' }} />)

    expect(screen.getByText('Waiting')).toBeDefined()
    expect(screen.queryByText('Queued')).toBeNull()

    rerender(<FeedItem item={{ ...baseItem, type: 'task.completed' }} />)

    expect(screen.getByText('Finished')).toBeDefined()
    expect(screen.queryByText('Completed')).toBeNull()
  })

  test('keeps unknown task updates readable', () => {
    render(<FeedItem item={{ ...baseItem, type: 'task.custom' }} />)

    expect(screen.getByText('Update')).toBeDefined()
    expect(screen.getByLabelText(/the agent reported a task update/i)).toBeDefined()
  })
})
