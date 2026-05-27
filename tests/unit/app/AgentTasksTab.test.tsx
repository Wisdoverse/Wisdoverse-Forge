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
        error: 'Import failed',
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
    expect(
      within(screen.getByTestId('agent-task-metric-needs-action')).getByText('2')
    ).toBeDefined()
    expect(within(screen.getByTestId('agent-task-metric-completed')).getByText('1')).toBeDefined()
    expect(screen.getByText('Needs help: Needs SSH key')).toBeDefined()
    expect(screen.getByText('Stopped because: Import failed')).toBeDefined()
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
    fireEvent.click(within(filters).getByRole('button', { name: /needs help\s*1/i }))

    expect(screen.getByText('Deploy service')).toBeDefined()
    expect(screen.queryByText('Build frontend')).toBeNull()
    expect(screen.queryByText('Review docs')).toBeNull()

    fireEvent.click(within(filters).getByRole('button', { name: /all\s*3/i }))
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
    fireEvent.click(screen.getByRole('button', { name: /done\s*0/i }))

    await waitFor(() => {
      expect(screen.getByTestId('agent-tasks-filter-empty')).toBeDefined()
    })
  })

  test('shows beginner next steps when the agent has no assigned tasks', async () => {
    getTasksByAgentMock.mockResolvedValue([])

    render(<AgentTasksTab agentId="agent-1" />)

    expect(
      await screen.findByText(
        'This agent has no assigned tasks yet. Assign a task to this agent to track the work here.'
      )
    ).toBeDefined()
  })
})
