import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { HistoryTab } from '@app/features/detail/HistoryTab'
import type { TaskSummary } from '@app/shared/api/orchestration'

const getTaskRunsMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      getTaskRuns: getTaskRunsMock,
    },
  }
})

afterEach(cleanup)

beforeEach(() => {
  getTaskRunsMock.mockReset()
})

function makeTask(overrides: Partial<TaskSummary> = {}): TaskSummary {
  return {
    id: 'task-1',
    state: 'working',
    method: 'tasks/send',
    params: { task: 'Review the deployment', message: '' },
    priority: 'normal',
    progress: 35,
    createdAt: '2026-04-25T06:00:00Z',
    updatedAt: '2026-04-25T06:05:00Z',
    assignedAgentName: 'Build Agent',
    ...overrides,
  }
}

describe('HistoryTab', () => {
  test('shows beginner recovery guidance when run attempts fail to load', async () => {
    getTaskRunsMock.mockRejectedValue(new Error('HTTP 403'))

    render(<HistoryTab task={makeTask()} />)

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(
      'You do not have permission to view this task. Ask an owner or admin to update your role.'
    )
    expect(alert.textContent).not.toContain('HTTP 403')
  })

  test('shows the empty attempt guidance after a successful empty load', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(<HistoryTab task={makeTask()} />)

    expect(
      await screen.findByText(/Attempts appear after an agent starts work/i)
    ).toBeInTheDocument()
  })
})
