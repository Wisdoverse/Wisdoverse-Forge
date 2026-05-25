import { describe, test, expect, afterEach } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { Workshop3DEmptyState } from '@app/widgets/views/Workshop3DView'

afterEach(cleanup)

describe('Workshop3DEmptyState', () => {
  test('guides first-time users before any agents are visible', () => {
    render(<Workshop3DEmptyState />)

    const emptyState = screen.getByTestId('workshop-3d-empty-state')

    expect(within(emptyState).getByText('No agents in the workshop yet')).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Create or wake an agent, then this view will show its live status and activity in 3D.'
      )
    ).toBeDefined()
    expect(within(emptyState).getByText('Create an agent from Agents')).toBeDefined()
    expect(within(emptyState).getByText('Start or wake the runtime')).toBeDefined()
    expect(within(emptyState).getByText('Refresh after the agent checks in')).toBeDefined()
  })
})
