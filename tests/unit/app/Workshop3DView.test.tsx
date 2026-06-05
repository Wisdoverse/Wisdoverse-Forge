import { describe, test, expect, afterEach } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import {
  Workshop3DEmptyState,
  Workshop3DStatusSummary,
} from '@app/widgets/views/Workshop3DView'

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

describe('Workshop3DStatusSummary', () => {
  test('uses beginner-safe labels instead of raw agent status words', () => {
    render(<Workshop3DStatusSummary totals={{ working: 2, idle: 1, offline: 0 }} />)

    expect(screen.getByText('2 Working')).toBeDefined()
    expect(screen.getByText('1 Ready')).toBeDefined()
    expect(screen.getByText('0 Offline')).toBeDefined()
    expect(screen.queryByText(/idle/i)).toBeNull()
  })
})
