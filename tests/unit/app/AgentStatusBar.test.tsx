import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { AgentStatusBar } from '@app/features/feed/AgentStatusBar'

afterEach(cleanup)

describe('AgentStatusBar', () => {
  test('guides first-time users when no agents are connected', () => {
    render(<AgentStatusBar agents={[]} />)

    const emptyState = screen.getByTestId('agent-status-empty')
    expect(emptyState).toBeDefined()
    expect(within(emptyState).getByText('Connect an agent before sending work')).toBeDefined()
    expect(
      within(emptyState).getByText('Open Agents and choose New agent if none exists.')
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Open Terminal on macOS/Linux or PowerShell on Windows, paste the setup text, and leave that window open.'
      )
    ).toBeDefined()
    expect(emptyState.textContent).not.toMatch(/command app/i)
    expect(
      within(emptyState).getByText('If an agent already exists, choose Start in Agents.')
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Success looks like one agent listed here as Ready or Working now.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Open Agents, then choose New agent/i)).toBeNull()
    expect(screen.queryByText(/before assigning work/i)).toBeNull()
    expect(screen.queryByText(/no agents are connected yet/i)).toBeNull()
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
    expect(within(statusBar).getByText('Handling a task')).toBeDefined()
    expect(within(statusBar).getByText('Waiting for work')).toBeDefined()
    expect(within(statusBar).getByText('Waiting for help')).toBeDefined()
    expect(within(statusBar).getByText('Start it in Agents')).toBeDefined()
    expect(within(statusBar).queryByText('Start or wake it')).toBeNull()
    expect(within(statusBar).queryByText('blocked')).toBeNull()
    expect(within(statusBar).queryByText('offline')).toBeNull()
  })

  test('describes what each status means for new operators', () => {
    render(
      <AgentStatusBar
        agents={[
          { id: 'idle', name: 'Reviewer', status: 'idle' },
          { id: 'blocked', name: 'Deployer', status: 'blocked' },
          { id: 'offline', name: 'Local host', status: 'offline' },
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
    expect(
      screen.getByLabelText(
        /local host: not connected\. open agents and choose connect this computer/i
      )
    ).toBeDefined()
    expect(screen.queryByLabelText(/this agent is not connected right now/i)).toBeNull()
    expect(screen.queryByLabelText(/clear a blocker/i)).toBeNull()
  })
})
