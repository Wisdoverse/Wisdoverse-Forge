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
  test('shows beginner recovery guidance when work history fails to load', async () => {
    getTaskRunsMock.mockRejectedValue(new Error('HTTP 403'))

    render(<HistoryTab task={makeTask()} />)

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(
      'You do not have permission to view this task. Ask an owner or admin to update your role.'
    )
    expect(alert.textContent).not.toContain('HTTP 403')
  })

  test('shows the empty work history guidance after a successful empty load', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(<HistoryTab task={makeTask()} />)

    expect(
      await screen.findByText(/Work history appears after an agent starts/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/Run attempts/i)).toBeNull()
  })

  test('labels work history by agent tool without worker jargon', async () => {
    getTaskRunsMock.mockResolvedValue([
      {
        id: 'run-12345678',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'running',
        startedAt: '2026-04-25T06:06:00Z',
        runtimeKind: 'container',
        cliTool: 'codex',
      },
    ])

    render(<HistoryTab task={makeTask()} />)

    expect(await screen.findByText('Agent work history')).toBeInTheDocument()
    expect(screen.getByText(/Used codex/i)).toBeInTheDocument()
    expect(screen.getByText(/Support reference run-1234/i)).toBeInTheDocument()
    expect(screen.queryByText(/^Ref run-1234$/i)).toBeNull()
    expect(screen.queryByText(/Work method|configured worker|unknown worker|runtime/i)).toBeNull()
  })

  test('summarizes failed task history without raw service details', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'failed',
          error: 'Rate limit exceeded: 429 from provider',
        })}
      />
    )

    expect(await screen.findAllByText(/model service is busy/i)).toHaveLength(2)
    expect(screen.queryByText(/429/)).toBeNull()
    expect(screen.queryByText(/provider/i)).toBeNull()
  })

  test('labels waiting task history without queue wording', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'queued',
          progress: 0,
        })}
      />
    )

    expect(await screen.findByText('Waiting to start')).toBeInTheDocument()
    expect(screen.getByText(/is waiting to start/i)).toBeInTheDocument()
    expect(screen.getByText(/waiting to begin/i)).toBeInTheDocument()
    expect(screen.queryByText('Queued')).toBeNull()
  })

  test('summarizes blocked task history without raw reason codes', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'blocked',
          blockedReason: 'quota_exceeded',
          error: 'quota_exceeded: docker socket denied secret token abc',
        })}
      />
    )

    expect(await screen.findAllByText(/Free capacity or ask an owner/i)).toHaveLength(2)
    expect(screen.queryByText(/quota_exceeded/i)).toBeNull()
    expect(screen.queryByText(/docker socket/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
  })
})
