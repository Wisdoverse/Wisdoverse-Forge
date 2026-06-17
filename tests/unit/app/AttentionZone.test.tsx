import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AttentionZone } from '@app/features/feed/AttentionZone'
import type { AttentionItem } from '@app/shared/model/feed.store'

const attentionItem: AttentionItem = {
  id: 'attention-1',
  taskTitle: 'Deploy staging',
  agentName: 'Agent Two',
  reason: 'Needs SSH key',
  timestamp: new Date('2026-05-25T12:00:00.000Z').getTime(),
}

afterEach(cleanup)

describe('AttentionZone', () => {
  test('guides first-time users before allowing work to continue', () => {
    render(<AttentionZone items={[attentionItem]} />)

    expect(screen.getByText('Needs your decision')).toBeDefined()
    expect(screen.getByText(/allow to continue only after checking/i)).toBeDefined()
    expect(screen.getByText('Deploy staging')).toBeDefined()
    expect(screen.getByText(/Agent Two is waiting: Waiting for account access/i)).toBeDefined()
    expect(screen.queryByText(/Needs SSH key/i)).toBeNull()
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

  test('keeps open and allow actions explicit', () => {
    const onView = vi.fn()
    const onApprove = vi.fn()

    render(<AttentionZone items={[attentionItem]} onView={onView} onApprove={onApprove} />)

    fireEvent.click(screen.getByRole('button', { name: /open details/i }))
    fireEvent.click(screen.getByRole('button', { name: /allow to continue/i }))

    expect(onView).toHaveBeenCalledWith('attention-1')
    expect(onApprove).toHaveBeenCalledWith('attention-1')
  })
})
