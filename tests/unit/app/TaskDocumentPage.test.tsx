import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import '@app/i18n'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { TaskDocumentPage } from '@app/pages/task-detail'

const { navigateSpy, getTask, getTaskRuns, getSelfFixReview } = vi.hoisted(() => ({
  navigateSpy: vi.fn(),
  getTask: vi.fn(),
  getTaskRuns: vi.fn(),
  getSelfFixReview: vi.fn(),
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
})
