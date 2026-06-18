import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AssignmentReadinessPanel } from '@app/features/board/AssignmentReadinessPanel'
import type { ParticipantSummary } from '@app/shared/api/orchestration'

afterEach(cleanup)

const emptyWorkload = {
  backlog: 0,
  unassigned: 0,
  inFlight: 0,
  blocked: 0,
  review: 0,
}

describe('AssignmentReadinessPanel', () => {
  test('guides setup when no agents are connected to the task queue', () => {
    render(
      <AssignmentReadinessPanel
        participants={[]}
        workload={{ ...emptyWorkload, backlog: 2, unassigned: 2 }}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const emptyState = screen.getByTestId('assignment-readiness-empty')
    expect(screen.getByRole('heading', { name: 'Agent status' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Refresh agent status' })).toBeDefined()
    expect(screen.getByText('Connect an agent before sending work.')).toBeDefined()
    expect(within(emptyState).getByText('Connect an agent before sending work')).toBeDefined()
    expect(within(emptyState).getByText(/Set up where tasks wait/)).toBeDefined()
    expect(within(emptyState).getByText(/add an available agent/i)).toBeDefined()
    expect(within(emptyState).getByText(/tasks that are not sent yet will wait here/)).toBeDefined()
    expect(within(emptyState).queryByText(/Open task queues/)).toBeNull()
    expect(screen.getByTestId('assignment-metric-backlog').textContent).toContain('Not sent yet')
    expect(screen.getByTestId('assignment-metric-unassigned').textContent).toContain('Needs agent')
    const previousActionPhrase = ['attach', 'an', 'available', 'agent'].join(' ')
    expect(emptyState.textContent).not.toContain(previousActionPhrase)
    const previousPanelTitle = ['Assignment', 'readiness'].join(' ')
    expect(screen.queryByText(previousPanelTitle)).toBeNull()
    expect(screen.queryByText(['Agent', 'readiness'].join(' '))).toBeNull()
    expect(
      screen.queryByRole('button', { name: ['Refresh', 'assignment', 'readiness'].join(' ') })
    ).toBeNull()
    expect(
      screen.queryByRole('button', { name: ['Refresh', 'agent', 'readiness'].join(' ') })
    ).toBeNull()
    expect(screen.queryByText('No agents are connected to this task queue yet.')).toBeNull()
    expect(emptyState.textContent).not.toContain('backlog')
    expect(emptyState.textContent).not.toContain('dispatch')
  })

  test('shows the next step when tasks need an available agent', () => {
    render(
      <AssignmentReadinessPanel
        participants={[
          {
            id: 'participant-1',
            agentId: 'agent-1',
            name: 'Ready Agent',
            status: 'available',
            capabilities: ['codex'],
          },
        ]}
        workload={{ ...emptyWorkload, backlog: 2, unassigned: 2 }}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const readiness = screen.getByTestId('assignment-readiness')
    expect(readiness.textContent).toContain(
      '2 tasks need an agent. Choose an available agent to start them.'
    )
    expect(screen.getByText('Can use Codex for this work')).toBeDefined()
    expect(readiness.textContent).not.toContain('codex')
    expect(readiness.textContent).not.toContain('unassigned tasks')
    expect(readiness.textContent).not.toContain('handed off')
    expect(readiness.textContent).not.toContain('can start now')
    expect(screen.getByTestId('assignment-metric-unassigned').textContent).toContain('Needs agent')
  })

  test('explains how to unblock tasks when no agent is available', () => {
    render(
      <AssignmentReadinessPanel
        participants={[
          {
            id: 'participant-1',
            agentId: 'agent-1',
            name: 'Busy Agent',
            status: 'busy',
            capabilities: ['codex'],
          },
        ]}
        workload={{ ...emptyWorkload, unassigned: 1 }}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const readiness = screen.getByTestId('assignment-readiness')
    expect(readiness.textContent).toContain(
      'Open Agents to start or connect an agent, or wait for one to finish.'
    )
    expect(readiness.textContent).toContain(
      '1 task needs an agent. Open Agents to start or connect an agent, or wait for one to finish.'
    )
    expect(screen.getByText('Already working · Can use Codex for this work')).toBeDefined()
    expect(readiness.textContent).not.toContain('codex')
    expect(readiness.textContent).not.toContain('unassigned tasks')
    expect(readiness.textContent).not.toContain('handed off')
    expect(readiness.textContent).not.toContain('No agent can take work right now')
    expect(readiness.textContent).not.toContain('Connect or free up an agent before it can start')
  })

  test('keeps connected agent chips visible when participants are available', () => {
    const participants: ParticipantSummary[] = [
      {
        id: 'participant-1',
        agentId: 'agent-1',
        name: 'Ready Agent',
        status: 'available',
        capabilities: ['codex'],
      },
    ]

    render(
      <AssignmentReadinessPanel
        participants={participants}
        workload={emptyWorkload}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    expect(screen.queryByTestId('assignment-readiness-empty')).toBeNull()
    expect(screen.getByText('Ready Agent')).toBeDefined()
    expect(screen.getAllByText('Can take work').length).toBeGreaterThan(0)
    expect(screen.getByText('Can use Codex for this work')).toBeDefined()
    expect(screen.getByText('Create a task when you have work to send.')).toBeDefined()
    expect(screen.queryByText('Task queue is clear.')).toBeNull()
  })

  test('summarizes tasks that need help without blocked-task wording', () => {
    render(
      <AssignmentReadinessPanel
        participants={[
          {
            id: 'participant-1',
            agentId: 'agent-1',
            name: 'Busy Agent',
            status: 'busy',
            capabilities: ['codex'],
          },
        ]}
        workload={{ ...emptyWorkload, blocked: 2 }}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const readiness = screen.getByTestId('assignment-readiness')
    expect(readiness.textContent).toContain('2 tasks need help before they can continue.')
    expect(screen.getByTestId('assignment-metric-blocked').textContent).toContain('Needs help')
    expect(readiness.textContent).not.toContain('blocked tasks')
    expect(readiness.textContent).not.toContain('Blocked')
  })

  test('uses plain offline agent activity labels', () => {
    const participants: ParticipantSummary[] = [
      {
        id: 'participant-1',
        agentId: 'agent-1',
        name: 'Recently Offline Agent',
        status: 'offline',
        capabilities: [],
        lastHeartbeatAt: new Date().toISOString(),
      },
      {
        id: 'participant-2',
        agentId: 'agent-2',
        name: 'Disconnected Agent',
        status: 'offline',
        capabilities: [],
      },
    ]

    render(
      <AssignmentReadinessPanel
        participants={participants}
        workload={emptyWorkload}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const readiness = screen.getByTestId('assignment-readiness')
    expect(within(readiness).getByText(/Last seen/i)).toBeDefined()
    expect(within(readiness).getByText('Open Agents to reconnect')).toBeDefined()
    expect(readiness.textContent).not.toContain('No recent activity')
    const previousStatusWord = ['heart', 'beat'].join('')
    expect(readiness.textContent).not.toContain(previousStatusWord)
  })
})
