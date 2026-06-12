import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { AgentStatusBar } from '@app/features/feed/AgentStatusBar'

afterEach(cleanup)

describe('AgentStatusBar', () => {
  test('guides first-time users when no agents are connected', () => {
    render(<AgentStatusBar agents={[]} />)

    expect(screen.getByTestId('agent-status-empty')).toBeDefined()
    expect(
      screen.getByText(/open agents to create or start one before assigning work/i)
    ).toBeDefined()
  })

  test('uses readable status labels instead of raw agent state values', () => {
    render(
      <AgentStatusBar
        agents={[
          { id: 'working', name: 'Builder', status: 'working' },
          { id: 'idle', name: 'Reviewer', status: 'idle' },
          { id: 'blocked', name: 'Deployer', status: 'blocked' },
          { id: 'offline', name: 'Local host', status: 'offline' },
        ]}
      />
    )

    const statusBar = screen.getByTestId('agent-status-bar')
    expect(within(statusBar).getByText('Working now')).toBeDefined()
    expect(within(statusBar).getByText('Ready')).toBeDefined()
    expect(within(statusBar).getByText('Needs help')).toBeDefined()
    expect(within(statusBar).getByText('Not connected')).toBeDefined()
    expect(within(statusBar).queryByText('blocked')).toBeNull()
    expect(within(statusBar).queryByText('offline')).toBeNull()
  })

  test('describes what each status means for new operators', () => {
    render(
      <AgentStatusBar
        agents={[
          { id: 'idle', name: 'Reviewer', status: 'idle' },
          { id: 'blocked', name: 'Deployer', status: 'blocked' },
        ]}
      />
    )

    expect(
      screen.getByLabelText(/reviewer: ready\. this agent is connected and waiting for work/i)
    ).toBeDefined()
    expect(
      screen.getByLabelText(
        /deployer: needs help\. this agent is waiting for help before it can continue/i
      )
    ).toBeDefined()
    expect(screen.queryByLabelText(/clear a blocker/i)).toBeNull()
  })
})
