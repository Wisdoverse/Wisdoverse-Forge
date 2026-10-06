import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import '@app/i18n'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { TaskDocumentPage } from '@app/pages/task-detail'

const {
  navigateSpy,
  getTask,
  getTaskRuns,
  getSelfFixReview,
  getParticipants,
  listTaskReviewChecks,
  setTaskReviewCheck,
  fetchTaskReviewGates,
  trackProductEvent,
} = vi.hoisted(() => ({
  navigateSpy: vi.fn(),
  getTask: vi.fn(),
  getTaskRuns: vi.fn(),
  getSelfFixReview: vi.fn(),
  getParticipants: vi.fn(),
  listTaskReviewChecks: vi.fn(),
  setTaskReviewCheck: vi.fn(),
  fetchTaskReviewGates: vi.fn(),
  trackProductEvent: vi.fn(),
}))

vi.mock('@app/shared/model/auth.context', () => ({
  useAuth: () => ({ user: null, isAuthenticated: false, isLoading: false }),
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
      trackProductEvent: (...args: unknown[]) => trackProductEvent(...args),
      getTask: (...args: unknown[]) => getTask(...args),
      getTaskRuns: (...args: unknown[]) => getTaskRuns(...args),
      getSelfFixReview: (...args: unknown[]) => getSelfFixReview(...args),
      getParticipants: (...args: unknown[]) => getParticipants(...args),
      listTaskReviewChecks: (...args: unknown[]) => listTaskReviewChecks(...args),
      setTaskReviewCheck: (...args: unknown[]) => setTaskReviewCheck(...args),
      fetchTaskReviewGates: (...args: unknown[]) => fetchTaskReviewGates(...args),
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
  listTaskReviewChecks.mockResolvedValue([])
  fetchTaskReviewGates.mockResolvedValue({ requiredKeys: [], satisfied: true, missing: [] })
  trackProductEvent.mockResolvedValue(undefined)
  setTaskReviewCheck.mockImplementation(
    async (_taskId: string, checkKey: string, done: boolean) => ({
      checkKey,
      done,
      updatedAt: new Date().toISOString(),
    })
  )
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

  test('opens and closes saved guidance from a completed task without losing its result', async () => {
    useBoardStore.getState().setTasks([
      seedTask({
        state: 'completed',
        progress: 100,
        result: [{ name: 'summary.md', mimeType: 'text/markdown', data: '## Delivered' }],
      }),
    ] as never)

    render(<TaskDocumentPage taskId="task-1" />)

    expect(trackProductEvent).not.toHaveBeenCalled()
    expect(await screen.findByRole('heading', { name: 'Delivered' })).toBeDefined()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Draft saved guidance' }))
    expect(await screen.findByRole('dialog', { name: 'Draft reusable guidance' })).toBeDefined()
    expect(trackProductEvent).toHaveBeenCalledWith('skill_draft_opened', {
      taskId: 'task-1',
      taskTitle: 'Fix the build',
    })
    expect(screen.getByText(/Remove passwords, access keys, customer data/i)).toBeDefined()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Close dialog' }))

    expect(screen.queryByRole('dialog', { name: 'Draft reusable guidance' })).toBeNull()
    expect(screen.getByRole('heading', { name: 'Delivered' })).toBeDefined()
  })

  test('closes the saved-guidance draft when the page opens another task', async () => {
    useBoardStore.getState().setTasks([
      seedTask({
        state: 'completed',
        result: [{ name: 'first.md', mimeType: 'text/markdown', data: '## First result' }],
      }),
      seedTask({
        id: 'task-2',
        state: 'completed',
        params: { task: 'Second task', message: 'second brief' },
        result: [{ name: 'second.md', mimeType: 'text/markdown', data: '## Second result' }],
      }),
    ] as never)

    const view = render(<TaskDocumentPage taskId="task-1" />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Draft saved guidance' }))
    expect(await screen.findByRole('dialog', { name: 'Draft reusable guidance' })).toBeDefined()

    view.rerender(<TaskDocumentPage taskId="task-2" />)

    expect(screen.queryByRole('dialog', { name: 'Draft reusable guidance' })).toBeNull()
    expect(await screen.findByRole('heading', { name: 'Second task' })).toBeDefined()
    expect(await screen.findByRole('heading', { name: 'Second result' })).toBeDefined()
  })

  test('keeps draft edits when the same task receives an update', async () => {
    const task = seedTask({ state: 'completed' })
    useBoardStore.getState().setTasks([task] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Draft saved guidance' }))
    const name = screen.getByRole('textbox', { name: 'Guidance name' })
    await user.clear(name)
    await user.type(name, 'Keep these edits')

    act(() => {
      useBoardStore.getState().upsertTask({
        ...task,
        progress: 100,
        params: { ...task.params, task: 'Updated task title' },
      } as never)
    })

    expect(await screen.findByRole('heading', { name: 'Updated task title' })).toBeDefined()
    expect(name).toHaveValue('Keep these edits')
    expect(trackProductEvent).toHaveBeenCalledTimes(1)
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

  test('shows attempt history in properties for a retried task', () => {
    useBoardStore
      .getState()
      .setTasks([seedTask({ state: 'failed', attempt: 3, error: 'boom' })] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    expect(screen.getByText('3 (retried 2 times)')).toBeDefined()
  })

  test('shows the review checklist with progress for a completed task', async () => {
    useBoardStore.getState().setTasks([seedTask({ state: 'completed', progress: 100 })] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    expect(screen.getByTestId('task-review-checklist')).toBeDefined()
    expect(await screen.findByText('0 of 4 checks done.')).toBeDefined()
  })

  test('marks required review gates and explains blocked acceptance', async () => {
    fetchTaskReviewGates.mockResolvedValue({
      requiredKeys: ['no_secrets', 'artifacts_checked'],
      satisfied: false,
      missing: ['no_secrets', 'artifacts_checked'],
    })
    useBoardStore.getState().setTasks([seedTask({ state: 'completed', progress: 100 })] as never)
    render(<TaskDocumentPage taskId="task-1" />)

    expect(await screen.findByTestId('review-gates-warning')).toHaveTextContent(
      '2 required check(s) still pending'
    )
    const checklist = screen.getByTestId('task-review-checklist')
    expect(checklist.textContent).toContain('Required')
  })

  test('shows all-clear when required review gates are satisfied', async () => {
    fetchTaskReviewGates.mockResolvedValue({
      requiredKeys: ['no_secrets'],
      satisfied: true,
      missing: [],
    })
    useBoardStore.getState().setTasks([seedTask({ state: 'completed', progress: 100 })] as never)
    render(<TaskDocumentPage taskId="task-1" />)

    expect(await screen.findByTestId('review-gates-satisfied')).toHaveTextContent(
      'All required checks are complete'
    )
    expect(screen.queryByTestId('review-gates-warning')).toBeNull()
  })

  test('tracks review checks optimistically and completes at 4 of 4', async () => {
    useBoardStore.getState().setTasks([seedTask({ state: 'completed', progress: 100 })] as never)
    render(<TaskDocumentPage taskId="task-1" />)
    await screen.findByText('0 of 4 checks done.')
    await waitFor(() =>
      expect(
        within(screen.getByTestId('review-check-result_matches_brief')).getByRole('checkbox')
      ).not.toBeDisabled()
    )
    const keys = ['result_matches_brief', 'artifacts_checked', 'no_secrets', 'reusable_saved']
    for (let index = 0; index < keys.length; index += 1) {
      const key = keys[index]
      await waitFor(() =>
        expect(
          within(screen.getByTestId(`review-check-${key}`)).getByRole('checkbox')
        ).not.toBeDisabled()
      )
      fireEvent.click(within(screen.getByTestId(`review-check-${key}`)).getByRole('checkbox'))
      await waitFor(() => expect(setTaskReviewCheck).toHaveBeenCalledTimes(index + 1))
    }
    expect(setTaskReviewCheck).toHaveBeenNthCalledWith(1, 'task-1', 'result_matches_brief', true)
    expect(screen.getByText('Review complete. Thanks for checking this work.')).toBeDefined()
  })
})
