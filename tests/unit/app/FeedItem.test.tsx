import { describe, expect, test } from 'vitest'
import { render, screen } from '@testing-library/react'
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

describe('FeedItem', () => {
  test('uses outcome labels that explain task updates to non-specialists', () => {
    render(
      <FeedItem item={{ ...baseItem, type: 'task.blocked', detail: 'Needs repository key' }} />
    )

    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.queryByText('Blocked')).toBeNull()
    expect(
      screen.getByLabelText(
        /needs help: builder on update checkout flow\. the task is waiting for someone to clear a blocker/i
      )
    ).toBeDefined()
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
