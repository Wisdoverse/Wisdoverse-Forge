import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AttentionZone } from '@app/features/feed/AttentionZone'
import type { AttentionItem } from '@app/entities/feed'

const attentionItem: AttentionItem = {
  id: 'attention-1',
  taskTitle: 'Deploy staging',
  agentName: 'Agent Two',
  reason: 'Needs SSH key',
  timestamp: new Date('2026-05-25T12:00:00.000Z').getTime(),
}

afterEach(cleanup)

describe('AttentionZone', () => {
  test('guides first-time users before clearing a decision item', () => {
    render(<AttentionZone items={[attentionItem]} />)

    expect(screen.getByText('Needs your decision')).toBeDefined()
    expect(screen.getByText(/mark checked only after opening the task/i)).toBeDefined()
    expect(
      screen.getByText(/waiting for a decision, missing access, or a quick check/i)
    ).toBeDefined()
    expect(screen.getByText('Deploy staging')).toBeDefined()
    expect(screen.getByText(/Agent Two is waiting: Waiting for account access/i)).toBeDefined()
    expect(screen.queryByText(/Needs SSH key/i)).toBeNull()
    expect(screen.queryByText(/quick review/i)).toBeNull()
  })

  test('renders safe attention reasons from the feed store', () => {
    render(
      <AttentionZone
        items={[
          {
            ...attentionItem,
            reason:
              'Waiting for account access. Add or reconnect the required service access, then retry.',
          },
        ]}
      />
    )

    expect(screen.getByText(/Agent Two is waiting: Waiting for account access/i)).toBeDefined()
    expect(screen.queryByText(/SSH key/i)).toBeNull()
  })

  test('keeps open and clear actions explicit', () => {
    const onView = vi.fn()
    const onDismiss = vi.fn()

    render(<AttentionZone items={[attentionItem]} onView={onView} onDismiss={onDismiss} />)

    fireEvent.click(screen.getByRole('button', { name: /open task details/i }))
    fireEvent.click(screen.getByRole('button', { name: /mark checked/i }))

    expect(onView).toHaveBeenCalledWith('attention-1')
    expect(onDismiss).toHaveBeenCalledWith('attention-1')
  })

  test('shows next-step feedback after an attention action', () => {
    render(
      <AttentionZone items={[attentionItem]} help="Open the task board, then check this task." />
    )

    expect(screen.getByRole('status')).toHaveTextContent('Open the task board')
  })
})
