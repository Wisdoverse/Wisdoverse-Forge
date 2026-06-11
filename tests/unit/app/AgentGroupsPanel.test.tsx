import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AgentGroupsPanel } from '@app/features/agents/AgentGroupsPanel'
import { useNavigationStore } from '@app/entities/navigation'
import { useBoardStore } from '@app/shared/model/board.store'
import type { TaskSummary } from '@app/shared/api/orchestration'

function makeTask(overrides: Partial<TaskSummary>): TaskSummary {
  return {
    id: 'task-default',
    groupId: 'group-delivery',
    state: 'backlog',
    method: 'agent.run',
    params: { task: 'Default task', message: '' },
    priority: 'normal',
    progress: 0,
    createdAt: '2026-05-24T08:00:00Z',
    updatedAt: '2026-05-24T08:00:00Z',
    ...overrides,
  }
}

function seedRoutingState(tasks: TaskSummary[]) {
  useNavigationStore.setState({
    selectedProjectId: 'project-1',
    projects: {
      'team-1': [
        {
          id: 'project-1',
          teamId: 'team-1',
          name: 'Platform',
          slug: 'platform',
          color: '#007AFF',
          description: '',
        },
      ],
    },
    agentGroups: [
      { id: 'group-delivery', projectId: 'project-1', name: 'Delivery Group' },
      { id: 'group-review', projectId: 'project-1', name: 'Review Group' },
    ],
  } as never)
  useBoardStore.getState().setSelectedGroupId('group-delivery')
  useBoardStore.getState().setTasks(tasks)
}

afterEach(() => {
  cleanup()
  useNavigationStore.getState().reset()
  useBoardStore.getState().reset()
})

describe('AgentGroupsPanel', () => {
  test('summarizes the selected task queue workload', () => {
    seedRoutingState([
      makeTask({
        id: 'backlog-1',
        state: 'backlog',
        params: { task: 'Plan billing', message: '' },
      }),
      makeTask({
        id: 'queued-1',
        state: 'queued',
        params: { task: 'Build settings page', message: '' },
      }),
      makeTask({
        id: 'working-1',
        state: 'working',
        params: { task: 'Wire provider health', message: '' },
        assignedAgentName: 'Build Agent',
        progress: 40,
      }),
      makeTask({
        id: 'blocked-1',
        state: 'blocked',
        params: { task: 'Auth handoff blocked', message: 'Needs reviewer' },
        blockedHint: 'Needs reviewer',
        priority: 'high',
      }),
      makeTask({
        id: 'failed-1',
        state: 'failed',
        params: { task: 'Retry deployment', message: '' },
        error: 'Rate limit exceeded: 429 from provider',
      }),
      makeTask({
        id: 'done-1',
        state: 'completed',
        params: { task: 'Document setup', message: '' },
        progress: 100,
      }),
      makeTask({
        id: 'review-other',
        groupId: 'group-review',
        state: 'working',
        params: { task: 'Other group work', message: '' },
      }),
    ])

    render(<AgentGroupsPanel />)

    expect(screen.getByTestId('task-routing-workload')).toBeInTheDocument()
    expect(within(screen.getByTestId('routing-metric-active')).getByText('2')).toBeInTheDocument()
    expect(within(screen.getByTestId('routing-metric-backlog')).getByText('1')).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-needs-action')).getByText('2')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-completed')).getByText('1')
    ).toBeInTheDocument()
    expect(screen.getByText('Auth handoff blocked')).toBeInTheDocument()
    expect(screen.getByText('Waiting to start')).toBeInTheDocument()
    expect(screen.getByText('Needs review')).toBeInTheDocument()
    expect(screen.queryByText('Queued')).not.toBeInTheDocument()
    expect(screen.queryByText('Failed')).not.toBeInTheDocument()
    expect(
      screen.getByText(/needs agent .* choose an agent before sending it/i)
    ).toBeInTheDocument()
    expect(screen.getByText(/build agent .* watch live progress/i)).toBeInTheDocument()
    expect(screen.getByText(/needs agent .* AI service is busy/i)).toBeInTheDocument()
    expect(screen.queryByText(/model service is busy/i)).toBeNull()
    expect(screen.queryByText(/dispatch/i)).toBeNull()
    expect(screen.queryByText(/monitor live progress/i)).toBeNull()
    expect(screen.queryByText(/runner/i)).toBeNull()
    expect(screen.queryByText(/429/)).toBeNull()
    expect(screen.queryByText(/rate limit exceeded/i)).toBeNull()
    expect(screen.queryByText(/from provider/i)).toBeNull()
    expect(screen.queryByText('Other group work')).toBeNull()
  })

  test('does not call routed work unassigned when only the agent id is loaded', () => {
    seedRoutingState([
      makeTask({
        id: 'queued-1',
        state: 'queued',
        params: { task: 'Deploy settings', message: '' },
        assignedTo: 'agent-1',
      }),
    ])

    render(<AgentGroupsPanel />)

    expect(screen.getByText('Deploy settings')).toBeInTheDocument()
    expect(
      screen.getByText(/assigned agent .* waiting for the next ready agent to start it/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/unassigned/i)).toBeNull()
  })

  test('filters the routed work queue by search', () => {
    seedRoutingState([
      makeTask({
        id: 'auth-1',
        state: 'blocked',
        params: { task: 'Auth handoff blocked', message: 'Needs reviewer' },
        blockedHint: 'Needs reviewer',
      }),
      makeTask({
        id: 'settings-1',
        state: 'working',
        params: { task: 'Build settings page', message: '' },
      }),
    ])

    render(<AgentGroupsPanel />)

    fireEvent.change(screen.getByTestId('task-routing-search'), {
      target: { value: 'auth' },
    })

    expect(screen.getByText('Auth handoff blocked')).toBeInTheDocument()
    expect(screen.queryByText('Build settings page')).toBeNull()

    fireEvent.change(screen.getByTestId('task-routing-search'), {
      target: { value: 'missing' },
    })

    expect(screen.getByTestId('task-routing-filter-empty')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /^clear$/i }))
    expect(screen.getByText('Build settings page')).toBeInTheDocument()
  })

  test('explains the next step when a task queue has no routed tasks', () => {
    seedRoutingState([])

    render(<AgentGroupsPanel />)

    const emptyState = screen.getByTestId('task-routing-empty')
    expect(emptyState).toHaveTextContent('No tasks are in this task queue yet')
    expect(emptyState).toHaveTextContent('Create a task and choose this task queue')
  })

  test('guides blank task queue names with examples', () => {
    seedRoutingState([])

    render(<AgentGroupsPanel />)

    fireEvent.click(screen.getByRole('button', { name: /new queue/i }))
    fireEvent.submit(screen.getByRole('button', { name: /create task queue/i }).closest('form')!)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Name this task queue before creating it. Examples: Intake, Review, or Delivery.'
    )
  })

  test('explains task queue creation permission failures with a next step', async () => {
    seedRoutingState([])
    const createAgentGroup = vi.fn().mockRejectedValue(new Error('HTTP 403: Forbidden'))
    useNavigationStore.setState({ createAgentGroup } as never)

    render(<AgentGroupsPanel />)

    fireEvent.click(screen.getByRole('button', { name: /new queue/i }))
    fireEvent.change(screen.getByLabelText(/task queue name/i), {
      target: { value: 'Delivery Queue' },
    })
    fireEvent.submit(screen.getByRole('button', { name: /create task queue/i }).closest('form')!)

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Task queue was not created. Ask an owner or admin to let you create and manage task queues in this project.'
    )
    expect(screen.queryByText(/HTTP 403/i)).toBeNull()
  })
})
