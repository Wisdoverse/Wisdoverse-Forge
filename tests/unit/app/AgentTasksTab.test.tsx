import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { AgentTasksTab } from '@app/features/agents/AgentTasksTab'
import type { TaskSummary } from '@app/shared/api/orchestration'

const getTasksByAgentMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: { getTasksByAgent: getTasksByAgentMock },
  }
})

afterEach(cleanup)

beforeEach(() => {
  getTasksByAgentMock.mockReset()
})

function makeTask(overrides: Partial<TaskSummary>): TaskSummary {
  return {
    id: 'task-default',
    state: 'backlog',
    method: 'tasks/send',
    params: { task: 'Default task', message: 'Default message' },
    priority: 'normal',
    progress: 0,
    createdAt: new Date(Date.now() - 60000).toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  }
}

describe('AgentTasksTab', () => {
  const previousEmptyNeedsHelpCopy = new RegExp(['Blocked', 'or', 'failed', 'work'].join('\\s+'))

  test('explains agent work loading for first-time users', () => {
    getTasksByAgentMock.mockImplementationOnce(() => new Promise(() => undefined))

    render(<AgentTasksTab agentId="agent-1" />)

    const loading = screen.getByRole('status', { name: /checking this agent's work list/i })
    expect(loading).toHaveTextContent("Checking this agent's work list")
    expect(loading).toHaveTextContent(
      'Forge is checking which tasks have been sent to this agent and which ones need your help.'
    )
    expect(loading).toHaveTextContent(
      'If this takes more than a moment, open this agent again or ask an owner or admin to check agent access.'
    )
    expect(loading).toHaveTextContent(
      'Success looks like active work, completed work, or a create-a-task step.'
    )
    expect(loading).not.toHaveTextContent("Checking this agent's work list...")
  })

  test('guides users when no tasks have reached the agent', async () => {
    getTasksByAgentMock.mockResolvedValue([])

    render(<AgentTasksTab agentId="agent-1" />)

    const emptyState = await screen.findByTestId('agent-tasks-empty')
    expect(within(emptyState).getByText('Create a task for this agent')).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Open the task list, choose New task, and pick this agent. Work will appear here after it is waiting or running.'
      )
    ).toBeDefined()
    const taskLink = within(emptyState).getByRole('link', { name: 'Open task list' })
    expect(taskLink).toHaveAttribute('href', '/tasks')
    expect(within(emptyState).getByText('Create a task')).toBeDefined()
    expect(within(emptyState).getByText('Check where tasks wait')).toBeDefined()
    expect(
      within(emptyState).getByText('Open where tasks wait, then make sure this agent is included.')
    ).toBeDefined()
    expect(within(emptyState).getByText('Use Needs help after tasks arrive')).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Work that needs help or stopped early appears there first, so you know what to fix.'
      )
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Success looks like a task showing Waiting to start or Doing now in this list.'
      )
    ).toBeDefined()
    expect(emptyState.textContent).not.toMatch(previousEmptyNeedsHelpCopy)
    expect(emptyState.textContent).not.toContain('Use Agents > Task Queues')
    expect(emptyState.textContent).not.toContain(
      'Send a small task to this agent, or choose where tasks wait'
    )
    expect(emptyState.textContent).not.toContain('task queue')
    expect(emptyState.textContent).not.toContain('routed')
    expect(emptyState.textContent).not.toContain('routing')
    expect(emptyState.textContent).not.toContain('Needs action')
    expect(emptyState.textContent).not.toContain('No tasks have reached this agent yet')
  })

  test('summarizes an agent task load', async () => {
    getTasksByAgentMock.mockResolvedValue([
      makeTask({ id: 'working', state: 'working', params: { task: 'Build API', message: '' } }),
      makeTask({ id: 'queued', state: 'queued', params: { task: 'Run tests', message: '' } }),
      makeTask({ id: 'backlog', state: 'backlog', params: { task: 'Draft plan', message: '' } }),
      makeTask({
        id: 'blocked',
        state: 'blocked',
        params: { task: 'Deploy service', message: '' },
        blockedHint: 'Needs SSH key',
      }),
      makeTask({
        id: 'failed',
        state: 'failed',
        params: { task: 'Import data', message: '' },
        error: 'Rate limit exceeded: 429 from provider',
      }),
      makeTask({
        id: 'completed',
        state: 'completed',
        params: { task: 'Review docs', message: '' },
      }),
    ])

    render(<AgentTasksTab agentId="agent-1" />)

    const summary = await screen.findByTestId('agent-task-workload')
    expect(within(summary).getByText('What this agent is handling')).toBeDefined()
    expect(within(summary).getByText('Needs help')).toBeDefined()
    expect(within(screen.getByTestId('agent-task-metric-active')).getByText('2')).toBeDefined()
    expect(within(screen.getByTestId('agent-task-metric-backlog')).getByText('1')).toBeDefined()
    expect(screen.getByText('Ready, not started')).toBeDefined()
    expect(
      screen.getByText('These tasks already have an agent, but work has not started yet.')
    ).toBeDefined()
    expect(screen.getByText('These tasks need a person to help them move forward.')).toBeDefined()
    expect(screen.getByText('Check retry steps')).toBeDefined()
    expect(
      screen.getByText('Open the task, check the latest update, then retry when ready.')
    ).toBeDefined()
    expect(screen.queryByText(/assigned.*not started/i)).toBeNull()
    expect(screen.queryByText(new RegExp(['unblock', 'them'].join('\\s+'), 'i'))).toBeNull()
    expect(screen.queryByText('Stopped with an error')).toBeNull()
    expect(screen.queryByText('These tasks stopped before finishing.')).toBeNull()
    const search = screen.getByRole('searchbox', { name: "Search this agent's work list" })
    expect(search).toHaveAccessibleDescription(
      "Search only filters this agent's work list. Use Show all agent work to return to the full list."
    )
    expect(screen.getByPlaceholderText('Search by task name, problem, or result')).toBe(search)
    expect(screen.queryByPlaceholderText(/blocker/i)).toBeNull()
    expect(
      within(screen.getByTestId('agent-task-metric-needs-action')).getByText('2')
    ).toBeDefined()
    expect(within(screen.getByTestId('agent-task-metric-completed')).getByText('1')).toBeDefined()
    const blockedPreview = screen.getByTestId('agent-task-blocked-blocked')
    expect(blockedPreview.textContent).toContain('Waiting for account access')
    expect(blockedPreview.textContent).not.toContain('SSH key')
    expect(blockedPreview.getAttribute('title')).toContain('Waiting for account access')
    expect(blockedPreview.getAttribute('title')).not.toContain('SSH key')
    const failedPreview = screen.getByTestId('agent-task-error-failed')
    expect(failedPreview.textContent).toContain('AI service is busy')
    expect(failedPreview.textContent).not.toContain('429')
    expect(failedPreview.textContent).not.toContain('provider')
    expect(failedPreview.textContent).not.toContain('model service is busy')
    expect(failedPreview.getAttribute('title')).toContain('AI service is busy')
  })

  test('shows beginner recovery guidance when agent tasks fail to load', async () => {
    getTasksByAgentMock.mockRejectedValue(new Error('HTTP 403'))

    render(<AgentTasksTab agentId="agent-1" />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(within(alert).getByText('Open Work again from this agent.')).toBeDefined()
    expect(alert.textContent).toContain(
      "Ask an owner or admin to give you access to this agent's work list."
    )
    expect(alert.textContent).not.toContain('HTTP 403')
    expect(alert.textContent).not.toContain('Details:')
    expect(alert.textContent).not.toContain("Refresh this agent's work list")
  })

  test('filters and searches tasks inside the agent profile', async () => {
    getTasksByAgentMock.mockResolvedValue([
      makeTask({
        id: 'blocked',
        state: 'blocked',
        params: { task: 'Deploy service', message: 'Release blocked' },
        blockedHint: 'Needs SSH key',
      }),
      makeTask({
        id: 'working',
        state: 'working',
        params: { task: 'Build frontend', message: 'Implement UI' },
        progress: 50,
      }),
      makeTask({
        id: 'completed',
        state: 'completed',
        params: { task: 'Review docs', message: 'Docs updated' },
      }),
    ])

    render(<AgentTasksTab agentId="agent-1" />)

    await screen.findByText('Deploy service')
    const filters = screen.getByTestId('agent-task-filter-group')
    fireEvent.click(
      within(filters).getByRole('button', {
        name: /show work that needs help or stopped early for this agent, 1 matching task/i,
      })
    )

    expect(screen.getByText('Deploy service')).toBeDefined()
    expect(screen.queryByText('Build frontend')).toBeNull()
    expect(screen.queryByText('Review docs')).toBeNull()

    fireEvent.click(
      within(filters).getByRole('button', {
        name: /show all work for this agent, 3 matching tasks/i,
      })
    )
    fireEvent.change(screen.getByTestId('agent-task-search'), { target: { value: 'frontend' } })

    expect(screen.getByText('Build frontend')).toBeDefined()
    expect(screen.queryByText('Deploy service')).toBeNull()
  })

  test('shows a filtered empty state', async () => {
    getTasksByAgentMock.mockResolvedValue([
      makeTask({
        id: 'working',
        state: 'working',
        params: { task: 'Build frontend', message: 'Implement UI' },
      }),
    ])

    render(<AgentTasksTab agentId="agent-1" />)

    await screen.findByText('Build frontend')
    fireEvent.click(
      screen.getByRole('button', { name: /show finished work for this agent, 0 matching tasks/i })
    )

    await waitFor(() => {
      expect(screen.getByTestId('agent-tasks-filter-empty')).toBeDefined()
    })
    const filterEmpty = screen.getByTestId('agent-tasks-filter-empty')
    expect(filterEmpty).toHaveAttribute('role', 'status')
    expect(filterEmpty).toHaveAttribute('aria-live', 'polite')
    expect(within(filterEmpty).getByText("Filter is hiding this agent's work")).toBeDefined()
    expect(filterEmpty.textContent).toContain('Use Show all agent work to return to the full list.')
    expect(filterEmpty.textContent).not.toContain('No tasks match this view')

    fireEvent.click(screen.getByRole('button', { name: /show all agent work/i }))

    expect(screen.getByText('Build frontend')).toBeDefined()
  })

  test('explains search-only and combined empty agent task filters', async () => {
    getTasksByAgentMock.mockResolvedValue([
      makeTask({
        id: 'working',
        state: 'working',
        params: { task: 'Build frontend', message: 'Implement UI' },
      }),
      makeTask({
        id: 'completed',
        state: 'completed',
        params: { task: 'Review docs', message: 'Docs updated' },
      }),
    ])

    render(<AgentTasksTab agentId="agent-1" />)

    await screen.findByText('Build frontend')
    fireEvent.change(screen.getByTestId('agent-task-search'), { target: { value: 'missing' } })

    const searchEmpty = screen.getByTestId('agent-tasks-filter-empty')
    expect(searchEmpty).toHaveAttribute('role', 'status')
    expect(searchEmpty).toHaveAttribute('aria-live', 'polite')
    expect(within(searchEmpty).getByText("Search is hiding this agent's work")).toBeDefined()
    expect(searchEmpty.textContent).toContain('Use Show all agent work to return to the full list.')
    expect(searchEmpty.textContent).not.toContain('No tasks match this view')

    fireEvent.click(screen.getByRole('button', { name: /show all agent work/i }))
    const filters = screen.getByTestId('agent-task-filter-group')
    fireEvent.click(
      within(filters).getByRole('button', {
        name: /show work that needs help or stopped early for this agent, 0 matching tasks/i,
      })
    )
    fireEvent.change(screen.getByTestId('agent-task-search'), { target: { value: 'frontend' } })

    const combinedEmpty = screen.getByTestId('agent-tasks-filter-empty')
    expect(combinedEmpty).toHaveAttribute('role', 'status')
    expect(combinedEmpty).toHaveAttribute('aria-live', 'polite')
    expect(within(combinedEmpty).getByText('Search and filter are hiding this work')).toBeDefined()
    expect(combinedEmpty.textContent).toContain(
      'Use Show all agent work before assuming this agent has no matching task.'
    )
    expect(combinedEmpty.textContent).not.toContain('No tasks match this view')
  })

  test('shows beginner next steps when the agent has no assigned tasks', async () => {
    getTasksByAgentMock.mockResolvedValue([])

    render(<AgentTasksTab agentId="agent-1" />)

    expect(await screen.findByText('Create a task for this agent')).toBeDefined()
    expect(screen.queryByText('No tasks have reached this agent yet')).toBeNull()
  })
})
