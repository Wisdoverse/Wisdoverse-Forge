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
  test('guides first-time users before approval', () => {
    render(<AttentionZone items={[attentionItem]} />)

    expect(screen.getByText('Needs your decision')).toBeDefined()
    expect(screen.getByText(/approve only after checking the request/i)).toBeDefined()
    expect(screen.getByText('Deploy staging')).toBeDefined()
    expect(screen.getByText(/Agent Two is waiting: Needs SSH key/i)).toBeDefined()
  })

  test('keeps review and approve actions explicit', () => {
    const onView = vi.fn()
    const onApprove = vi.fn()

    render(<AttentionZone items={[attentionItem]} onView={onView} onApprove={onApprove} />)

    fireEvent.click(screen.getByRole('button', { name: /review request/i }))
    fireEvent.click(screen.getByRole('button', { name: /approve request/i }))

    expect(onView).toHaveBeenCalledWith('attention-1')
    expect(onApprove).toHaveBeenCalledWith('attention-1')
  })
})
