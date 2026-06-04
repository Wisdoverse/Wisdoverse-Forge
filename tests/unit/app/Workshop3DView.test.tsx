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
        'If this is your first agent, create it from Agents. If you already have one, start or wake it there, then refresh this view.'
      )
    ).toBeDefined()
    expect(within(emptyState).getByText('Open Agents and create one if none exists')).toBeDefined()
    expect(
      within(emptyState).getByText('Start or wake the agent if it is already listed')
    ).toBeDefined()
    expect(
      within(emptyState).getByText('Refresh this view after the agent checks in')
    ).toBeDefined()
  })
})
