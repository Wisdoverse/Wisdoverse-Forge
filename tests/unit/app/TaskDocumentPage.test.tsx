import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import '@app/i18n'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { TaskDocumentPage } from '@app/pages/task-detail'

const { navigateSpy, getTask, getTaskRuns, getSelfFixReview, getParticipants } = vi.hoisted(() => ({
  navigateSpy: vi.fn(),
  getTask: vi.fn(),
  getTaskRuns: vi.fn(),
  getSelfFixReview: vi.fn(),
  getParticipants: vi.fn(),
}))

vi.mock('@tanstack/react-router', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tanstack/react-router')>()),
  useNavigate: () => navigateSpy,
}))

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      getTask: (...args: unknown[]) => getTask(...args),
      getTaskRuns: (...args: unknown[]) => getTaskRuns(...args),
      getSelfFixReview: (...args: unknown[]) => getSelfFixReview(...args),
      getParticipants: (...args: unknown[]) => getParticipants(...args),
    },
  }
})

function seedTask(overrides: Record<string, unknown> = {}) {
  return {
    id: 'task-1',
    state: 'working',
    method: 'work',
    params: { task: 'Fix the build', message: '# Brief\n\ndo it' },
    priority: 'normal',
    progress: 40,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    attempt: 1,
    ...overrides,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  useBoardStore.getState().reset()
  getTaskRuns.mockResolvedValue([])
  getParticipants.mockResolvedValue([])
  getSelfFixReview.mockResolvedValue({
    taskId: 'task-1',
    prNumber: 42,
    prUrl: 'https://github.com/o/r/pull/42',
    diffUrl: 'https://github.com/o/r/pull/42/files',
    headSha: 'deadbeef',
    checksGreen: true,
    sensitive: false,
    reviewStatus: 'in_review',
  })
})

afterEach(() => {
  cleanup()
})

describe('TaskDocumentPage', () => {
  test('renders title and breadcrumb from the board store', () => {
    useBoardStore.getState().setTasks([seedTask()] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    expect(screen.getByRole('heading', { level: 1, name: 'Fix the build' })).toBeDefined()
    expect(screen.getByRole('navigation', { name: /breadcrumb/i })).toBeDefined()
    expect(getTask).not.toHaveBeenCalled()
  })

  test('fetches on cold deep link and renders the task', async () => {
    getTask.mockResolvedValue(seedTask())
    render(<TaskDocumentPage taskId="task-1" />)
    expect(screen.getByTestId('task-document-loading')).toBeDefined()
    await waitFor(() =>
      expect(screen.getByRole('heading', { level: 1, name: 'Fix the build' })).toBeDefined()
    )
  })

  test('shows a beginner-first missing state with a way back', async () => {
    getTask.mockRejectedValue(new Error('API 404: {"error":"task not found"}'))
    render(<TaskDocumentPage taskId="nope" />)
    await waitFor(() =>
      expect(screen.getByText('This task is not on the board anymore.')).toBeDefined()
    )
    expect(screen.getByRole('button', { name: 'Open the task board' })).toBeDefined()
  })

  test('shows the review section only for self-fix tasks', () => {
    useBoardStore.getState().setTasks([seedTask({ selfFix: true })] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    expect(screen.getByTestId('review-snapshot-panel')).toBeDefined()
  })

  test('renders the activity footer', () => {
    useBoardStore.getState().setTasks([seedTask()] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    expect(screen.getByTestId('task-updates')).toBeDefined()
  })

  test('keeps assignment guidance for a backlog task without an agent', () => {
    useBoardStore
      .getState()
      .setTasks([
        seedTask({ state: 'backlog', params: { task: 'Fix the build', message: 'Do it' } }),
      ] as never)

    render(<TaskDocumentPage taskId="task-1" />)

    const assignment = within(screen.getByRole('region', { name: 'Assignment' }))
    expect(assignment.getByText('Needs agent')).toBeDefined()
    expect(assignment.getByTestId('task-assignment-guidance')).toHaveTextContent(
      'Choose an agent before this task can start.'
    )
    expect(assignment.getByRole('link', { name: 'Open Agents' })).toHaveAttribute('href', '/agents')
  })

  test('keeps assignment guidance when only the agent id has loaded', () => {
    useBoardStore.getState().setTasks([
      seedTask({
        state: 'backlog',
        assignedTo: 'agent-1',
        params: { task: 'Fix the build', message: 'Do it' },
      }),
    ] as never)

    render(<TaskDocumentPage taskId="task-1" />)

    const assignment = within(screen.getByRole('region', { name: 'Assignment' }))
    expect(assignment.getByText('Loading agent name')).toBeDefined()
    expect(assignment.getByTestId('task-assignment-guidance')).toHaveTextContent(
      'An agent was chosen, but its name has not loaded yet. Open this task again so you can confirm the right agent before sending it.'
    )
    expect(screen.queryByText('Unassigned')).toBeNull()
  })

  test('keeps completed assignment, result, and handoff guidance together', async () => {
    useBoardStore.getState().setTasks([
      seedTask({
        state: 'completed',
        progress: 100,
        assignedAgentName: 'Review Agent',
        result: [{ name: 'summary.md', mimeType: 'text/markdown', data: '## Delivered' }],
      }),
    ] as never)

    render(<TaskDocumentPage taskId="task-1" />)

    expect(screen.getByTestId('task-assignment-guidance')).toHaveTextContent(
      'This agent finished this task. Check the result before accepting it.'
    )
    expect(await screen.findByRole('heading', { name: 'Delivered' })).toBeDefined()
    expect(screen.getByTestId('task-handoff-checklist')).toBeDefined()
  })

  test('turns missing brief and result files into next steps', () => {
    useBoardStore.getState().setTasks([
      seedTask({
        state: 'completed',
        progress: 100,
        params: { task: 'Fix the build', message: '' },
      }),
    ] as never)

    render(<TaskDocumentPage taskId="task-1" />)

    expect(screen.getByTestId('task-brief-empty')).toHaveTextContent(
      'No brief was saved. Open Updates to see what was asked before accepting, retrying, or closing this task.'
    )
    expect(screen.getByTestId('task-result-empty')).toHaveTextContent(
      'No result files were saved. Use Next action above, then retry or create a follow-up task if files are still needed.'
    )
  })

  test('summarizes blocked assignment hints without exposing service details', () => {
    useBoardStore.getState().setTasks([
      seedTask({
        state: 'blocked',
        blockedReason: 'waiting_input',
        blockedHint: 'Needs API token secret for registry access',
        error: 'registry auth failed with token secret',
      }),
    ] as never)

    render(<TaskDocumentPage taskId="task-1" />)

    expect(screen.getByTestId('task-assignment-blocked-guidance')).toHaveTextContent(
      'Waiting for account access'
    )
    expect(screen.queryByText(/API token secret|registry auth/i)).toBeNull()
  })
})
