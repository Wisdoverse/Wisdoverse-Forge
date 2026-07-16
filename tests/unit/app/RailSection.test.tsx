import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import { RailSection, RailRow } from '@app/features/detail/rail/RailSection'
import { DetailsGroup } from '@app/features/detail/rail/DetailsGroup'

describe('RailSection', () => {
  test('toggles content with an accessible disclosure', () => {
    render(
      <RailSection title="Details">
        <RailRow label="Created">now</RailRow>
      </RailSection>
    )
    const toggle = screen.getByRole('button', { name: 'Details' })
    expect(toggle.getAttribute('aria-expanded')).toBe('true')
    expect(screen.getByText('now')).toBeDefined()
    fireEvent.click(toggle)
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByText('now')).toBeNull()
  })
})

describe('DetailsGroup', () => {
  test('omits rows with no data instead of placeholders', () => {
    render(
      <DetailsGroup
        task={
          {
            id: 't',
            state: 'queued',
            method: 'work',
            params: { task: 'x', message: '' },
            priority: 'normal',
            progress: 0,
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
            attempt: 1,
          } as never
        }
      />
    )
    expect(screen.queryByText('Created by')).toBeNull()
    expect(screen.getByText('Created')).toBeDefined()
  })
})
