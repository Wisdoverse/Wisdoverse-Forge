import { describe, test, expect, afterEach } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import type { AgentInfo } from '@app/entities/agent'
import {
  Workshop3DEmptyState,
  Workshop3DInteractionHint,
  Workshop3DStatusSummary,
  workshop3DAgentSubtitle,
} from '@app/widgets/views/Workshop3DView'

afterEach(cleanup)

describe('Workshop3DEmptyState', () => {
  test('guides first-time users before any agents are visible', () => {
    render(<Workshop3DEmptyState />)

    const emptyState = screen.getByTestId('workshop-3d-empty-state')

    expect(within(emptyState).getByText('Open Agents to build the visual map')).toBeDefined()
    expect(
      within(emptyState).getByText(
        'If this is your first agent, create it from Agents. If you already have one, start or wake it there, then refresh this view.'
      )
    ).toBeDefined()
    expect(within(emptyState).queryByText('No agents on the visual map yet')).toBeNull()
    expect(within(emptyState).queryByText(/workshop/i)).toBeNull()
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

    expect(screen.getByText('2 Working now')).toBeDefined()
    expect(screen.getByText('1 Ready')).toBeDefined()
    expect(screen.getByText('0 Not connected')).toBeDefined()
    expect(screen.queryByText(/idle/i)).toBeNull()
  })
})

describe('Workshop3DInteractionHint', () => {
  test('tells first-time users how to select an agent without unsupported mouse controls', () => {
    render(<Workshop3DInteractionHint />)

    const hint = screen.getByTestId('workshop-3d-interaction-hint')

    expect(hint.textContent).toContain('Choose an agent from the list')
    expect(hint.textContent).toContain('select a robot in the map')
    expect(hint.textContent).not.toContain('Middle-click')
    expect(hint.textContent).not.toContain('Right-click')
    expect(hint.textContent).not.toContain('Scroll to zoom')
  })
})

describe('workshop3DAgentSubtitle', () => {
  test('uses beginner-safe runtime labels instead of raw service fields', () => {
    const agent: AgentInfo = {
      id: 'agent-1',
      name: 'Planning agent',
      provider: 'openai_compatible',
      model: 'vendor-internal-model',
      status: 'idle',
      tasksCompleted: 0,
      tasksInProgress: 0,
      successRate: 0,
      cliTool: 'codex',
      runtimeKind: 'cli',
    }

    expect(workshop3DAgentSubtitle(agent)).toBe('Ready - Codex on this computer')
    expect(workshop3DAgentSubtitle(agent)).not.toContain('vendor-internal-model')
    expect(workshop3DAgentSubtitle(agent)).not.toContain('openai_compatible')
  })
})
