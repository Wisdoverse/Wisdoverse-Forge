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
      { id: 'group-delivery', projectId: 'project-1', name: 'Delivery Queue' },
      { id: 'group-review', projectId: 'project-1', name: 'Review Queue' },
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
  const previousBlockedLabel = ['Block', 'ed'].join('')
  const previousBlockingCopy = new RegExp(['what', 'is', 'blocking'].join('\\s+'), 'i')

  test('routes users to project settings before setting up where tasks wait', () => {
    const onOpenProjectsSetup = vi.fn()

    render(<AgentGroupsPanel onOpenProjectsSetup={onOpenProjectsSetup} />)

    const panel = screen.getByTestId('agent-groups-panel')
    expect(panel).toHaveTextContent(/where tasks wait/i)
    expect(panel).toHaveTextContent(/shared waiting places tell agents where to start/i)
    expect(panel).not.toHaveTextContent(/task queues/i)
    expect(panel).not.toHaveTextContent(/agents check for tasks/i)
    expect(panel).not.toHaveTextContent(/pick up/i)
    expect(panel).toHaveTextContent(/open project settings to create a project/i)
    expect(panel).not.toHaveTextContent(/select a project from the sidebar/i)

    fireEvent.click(within(panel).getByRole('button', { name: /open project settings/i }))

    expect(onOpenProjectsSetup).toHaveBeenCalledTimes(1)
  })

  test('summarizes the selected waiting place workload', () => {
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
    expect(screen.getByText('Tasks waiting here')).toBeInTheDocument()
    expect(screen.getAllByText('Delivery waiting place').length).toBeGreaterThan(0)
    expect(screen.queryByText('Delivery Queue')).toBeNull()
    expect(screen.getByText('6 tasks here')).toBeInTheDocument()
    expect(within(screen.getByTestId('routing-metric-active')).getByText('2')).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-active')).getByText('Working now')
    ).toBeInTheDocument()
    expect(within(screen.getByTestId('routing-metric-backlog')).getByText('1')).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-needs-action')).getByText('2')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-needs-action')).getByText('Needs help')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-completed')).getByText('1')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('routing-metric-completed')).getByText('Done')
    ).toBeInTheDocument()
    expect(screen.getByText('Auth handoff blocked')).toBeInTheDocument()
    const routedRows = screen.getAllByTestId('task-routing-row')
    expect(within(routedRows[0]).getByText('Needs help')).toBeInTheDocument()
    expect(screen.getByText('Waiting to start')).toBeInTheDocument()
    expect(screen.getAllByText('Not sent yet').length).toBeGreaterThan(0)
    expect(screen.getByText('Check retry steps')).toBeInTheDocument()
    expect(screen.queryByText(previousBlockedLabel)).not.toBeInTheDocument()
    expect(screen.queryByText('Backlog')).not.toBeInTheDocument()
    expect(screen.queryByText('Queued')).not.toBeInTheDocument()
    expect(screen.queryByText('Failed')).not.toBeInTheDocument()
    expect(screen.queryByText(/routed/i)).toBeNull()
    expect(screen.getByPlaceholderText('Search tasks, agents, or problems...')).toBeDefined()
    expect(screen.queryByPlaceholderText(/blockers/i)).toBeNull()
    expect(screen.queryByPlaceholderText(new RegExp(['assig', 'nees'].join(''), 'i'))).toBeNull()
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
    expect(screen.getByLabelText('Search tasks in this waiting place')).toHaveAccessibleDescription(
      'Search only filters tasks in this waiting place. Use Show all tasks here to return to the full waiting place.'
    )
  })

  test('describes completed task result checks without handoff wording', () => {
    seedRoutingState([
      makeTask({
        id: 'done-1',
        state: 'completed',
        params: { task: 'Document setup', message: '' },
        assignedAgentName: 'Docs Agent',
        progress: 100,
      }),
    ])

    render(<AgentGroupsPanel />)

    expect(screen.getByText(/docs agent .* check the finished result/i)).toBeInTheDocument()
    expect(screen.queryByText(/review what the agent finished/i)).toBeNull()
    expect(screen.queryByText(new RegExp(['completed', 'handoff'].join('\\s+'), 'i'))).toBeNull()
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
      screen.getByText(/chosen agent .* waiting for an available agent to start it/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/assigned agent/i)).toBeNull()
    expect(screen.queryByText(/unassigned/i)).toBeNull()
  })

  test('shows code access wording and hides sensitive blocked hints in routed task next steps', () => {
    seedRoutingState([
      makeTask({
        id: 'blocked-credentials',
        state: 'blocked',
        params: { task: 'Connect repository access', message: '' },
        blockedHint: 'Missing token secret for git provider.',
      }),
    ])

    render(<AgentGroupsPanel />)

    expect(screen.getByText('Connect code access')).toBeInTheDocument()
    expect(screen.queryByText('Connect repository access')).toBeNull()
    expect(screen.getByText(/needs agent .* waiting for account access/i)).toBeInTheDocument()
    expect(screen.queryByText(/token secret/i)).toBeNull()
    expect(screen.queryByText(/git provider/i)).toBeNull()
  })

  test('filters the waiting place by search', () => {
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

    const search = screen.getByLabelText('Search tasks in this waiting place')
    expect(search).toHaveAccessibleDescription(
      'Search only filters tasks in this waiting place. Use Show all tasks here to return to the full waiting place.'
    )

    fireEvent.change(search, {
      target: { value: 'auth' },
    })

    expect(screen.getByText('Auth handoff blocked')).toBeInTheDocument()
    expect(screen.queryByText('Build settings page')).toBeNull()

    fireEvent.change(search, {
      target: { value: 'missing' },
    })

    const emptyState = screen.getByTestId('task-routing-filter-empty')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(
      within(emptyState).getByText('Search is hiding tasks in this waiting place')
    ).toBeInTheDocument()
    expect(within(emptyState).getByText(/this waiting place still has tasks/i)).toBeInTheDocument()
    expect(within(emptyState).getByText(/before assuming this place is empty/i)).toBeInTheDocument()
    expect(emptyState.textContent).not.toContain('No tasks in this task queue match this search.')
    expect(within(emptyState).queryByRole('button', { name: /^clear$/i })).toBeNull()

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all tasks here/i }))
    expect(screen.getByTestId('task-routing-search')).toHaveValue('')
    expect(screen.getByText('Build settings page')).toBeInTheDocument()
  })

  test('does not match hidden agent ids in waiting place search', () => {
    seedRoutingState([
      makeTask({
        id: 'hidden-agent-task',
        state: 'queued',
        params: { task: 'Prepare customer handoff', message: 'Summarize the next step' },
        assignedTo: 'agent-hidden-42',
      }),
    ])

    render(<AgentGroupsPanel />)

    expect(screen.getByText('Prepare customer handoff')).toBeInTheDocument()
    expect(screen.getByText(/chosen agent .* waiting for an available agent/i)).toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('Search tasks in this waiting place'), {
      target: { value: 'agent-hidden-42' },
    })

    const emptyState = screen.getByTestId('task-routing-filter-empty')
    expect(emptyState).toHaveTextContent('Search is hiding tasks in this waiting place')
    expect(screen.queryByText('Prepare customer handoff')).toBeNull()
  })

  test('explains the next step when a waiting place has no routed tasks', () => {
    seedRoutingState([])

    render(<AgentGroupsPanel />)

    const emptyState = screen.getByTestId('task-routing-empty')
    expect(emptyState).toHaveTextContent('Create the first task for this waiting place')
    expect(emptyState).toHaveTextContent('then choose it')
    expect(emptyState).toHaveTextContent(
      'Success looks like a task showing Waiting to start or Working here.'
    )
    expect(emptyState).not.toHaveTextContent('No tasks are in this task queue yet')
  })

  test('guides blank waiting place names with examples', () => {
    seedRoutingState([])

    render(<AgentGroupsPanel />)

    fireEvent.click(screen.getByRole('button', { name: /^set up waiting place$/i }))
    expect(screen.getByRole('group', { name: /waiting place templates/i })).toBeInTheDocument()
    fireEvent.click(screen.getByText('Build and verify').closest('button')!)
    expect(screen.getByLabelText(/waiting place description/i)).toHaveValue(
      'Build the requested changes, keep work moving, and run checks before sharing results.'
    )
    expect(screen.queryByDisplayValue(/scoped changes/i)).toBeNull()
    expect(screen.queryByDisplayValue(/handoff/i)).toBeNull()
    const resultCheckSummary = screen.getByText('Check before use')
    const triageSummary = screen.getByText('Clarify and send')
    fireEvent.click(triageSummary.closest('button')!)
    expect(screen.getByRole('button', { name: /sort work/i })).toBeInTheDocument()
    expect(screen.getByLabelText(/waiting place name/i)).toHaveValue('Intake Tasks')
    expect(screen.getByLabelText(/waiting place description/i)).toHaveValue(
      'Clarify incoming work, find what is missing, and send tasks to the right agent.'
    )
    expect(screen.queryByDisplayValue(/queue/i)).toBeNull()
    expect(screen.queryByDisplayValue(/triage/i)).toBeNull()
    expect(screen.queryByDisplayValue(previousBlockingCopy)).toBeNull()
    fireEvent.click(resultCheckSummary.closest('button')!)
    expect(resultCheckSummary).toBeInTheDocument()
    expect(screen.queryByText(['Risk', 'and', 'readiness'].join(' '))).toBeNull()
    expect(screen.getByLabelText(/waiting place name/i)).toHaveValue('Result Check Tasks')
    fireEvent.click(resultCheckSummary.closest('button')!)
    expect(screen.getByLabelText(/waiting place description/i)).toHaveValue(
      'Check finished work for confusing behavior, missing checks, and anything that could make it unsafe to use.'
    )
    expect(screen.queryByDisplayValue(/missing tests/i)).toBeNull()
    expect(screen.queryByDisplayValue(/Review completed work/i)).toBeNull()
    expect(screen.queryByDisplayValue(/block release/i)).toBeNull()
    fireEvent.change(screen.getByLabelText(/waiting place name/i), { target: { value: '' } })
    fireEvent.submit(screen.getByRole('button', { name: /create waiting place/i }).closest('form')!)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Name this waiting place before creating it. Examples: Intake, Result Check, or Delivery.'
    )
    expect(screen.getByLabelText(/waiting place name/i)).toHaveFocus()

    fireEvent.change(screen.getByLabelText(/waiting place name/i), {
      target: { value: 'Intake Tasks' },
    })
    expect(screen.queryByRole('alert')).toBeNull()
  })

  test('explains waiting place creation permission failures with a next step', async () => {
    seedRoutingState([])
    const createAgentGroup = vi.fn().mockRejectedValue(new Error('HTTP 403: Forbidden'))
    useNavigationStore.setState({ createAgentGroup } as never)

    render(<AgentGroupsPanel />)

    fireEvent.click(screen.getByRole('button', { name: /^set up waiting place$/i }))
    expect(screen.getByRole('button', { name: 'Create waiting place' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Create Task Queue' })).toBeNull()
    fireEvent.change(screen.getByLabelText(/waiting place name/i), {
      target: { value: 'Delivery Tasks' },
    })
    fireEvent.change(screen.getByLabelText(/waiting place description/i), {
      target: { value: 'Keep delivery tasks moving.' },
    })
    fireEvent.submit(screen.getByRole('button', { name: /create waiting place/i }).closest('form')!)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Ask an owner or admin to let you set up where tasks wait in this project. The waiting place was not created.'
    )
    expect(screen.getByLabelText(/waiting place name/i)).toHaveValue('Delivery Tasks')
    expect(screen.getByLabelText(/waiting place description/i)).toHaveValue(
      'Keep delivery tasks moving.'
    )
    expect(screen.queryByText(/HTTP 403/i)).toBeNull()
  })
})
