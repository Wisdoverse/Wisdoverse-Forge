import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AssignmentReadinessPanel } from '@app/features/board/AssignmentReadinessPanel'

const emptyWorkload = {
  backlog: 2,
  unassigned: 1,
  inFlight: 0,
  blocked: 0,
  review: 0,
}

const healthyWorkload = {
  backlog: 0,
  unassigned: 0,
  inFlight: 1,
  blocked: 0,
  review: 0,
}

afterEach(() => {
  cleanup()
})

describe('AssignmentReadinessPanel', () => {
  test('keeps healthy readiness compact before the task cards', () => {
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
        workload={healthyWorkload}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const readiness = screen.getByTestId('assignment-readiness')

    expect(readiness).toHaveTextContent('1 agent can take work now.')
    expect(screen.queryByTestId('assignment-metric-backlog')).toBeNull()
    expect(screen.queryByText('Ready Agent')).toBeNull()
  })

  test('explains missing agents without queue-state wording', () => {
    render(
      <AssignmentReadinessPanel
        participants={[]}
        workload={emptyWorkload}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const emptyState = screen.getByTestId('assignment-readiness-empty')

    expect(
      within(emptyState).getByText(
        'Set up a place for new tasks in this project, then add or start an agent. Until then, new tasks wait on this board.'
      )
    ).toBeDefined()
    expect(emptyState).not.toHaveTextContent('task queue')
    expect(emptyState).not.toHaveTextContent('where tasks wait')
    expect(emptyState).not.toHaveTextContent('available agent')
    expect(emptyState).not.toHaveTextContent('Not sent yet')
    expect(emptyState).not.toHaveTextContent('ready agent')
  })

  test('does not count chat-only agents as able to take Tasks', () => {
    render(
      <AssignmentReadinessPanel
        participants={[
          {
            id: 'participant-1',
            agentId: 'agent-1',
            name: 'Chat Helper',
            status: 'available',
            capabilities: [],
          },
        ]}
        workload={emptyWorkload}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
      />
    )

    const readiness = screen.getByTestId('assignment-readiness')

    expect(readiness).toHaveTextContent(
      'Simple chat agents answer in Chat. For Tasks, add an agent with Project files or This computer.'
    )
    expect(readiness).toHaveTextContent(
      '1 task needs an agent. Open Agents to start or connect an agent, or wait for one to finish.'
    )
    expect(screen.getByTestId('assignment-metric-can-take-work')).toHaveTextContent('0')
    expect(screen.getByTestId('assignment-metric-questions-only')).toHaveTextContent('1')
    expect(screen.getByText('Chat Helper')).toBeDefined()
    expect(screen.getAllByText('Questions only').length).toBeGreaterThanOrEqual(1)
    expect(screen.queryByText('Chat only')).toBeNull()
    expect(
      screen.getByText(
        'Simple chat only. Use an agent with Project files or This computer for Tasks.'
      )
    ).toBeDefined()
    expect(readiness).not.toHaveTextContent('Add a Project files or This computer agent')
    expect(readiness).not.toHaveTextContent('1 agent can take work now.')
  })
})
