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
  test('gives assigned backlog tasks a direct start step', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(<HistoryTab task={makeTask({ state: 'backlog', progress: 0 })} />)

    expect(
      await screen.findByText('The task has an agent. Review the brief, then start the task.')
    ).toBeInTheDocument()
    expect(screen.queryByText(new RegExp(['brief', 'is', 'ready'].join('\\s+'), 'i'))).toBeNull()
    expect(screen.queryByText(/when ready/i)).toBeNull()
  })

  test('shows beginner recovery guidance when work history fails to load', async () => {
    getTaskRunsMock.mockRejectedValue(new Error('HTTP 403'))

    render(<HistoryTab task={makeTask()} />)

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain(
      'You do not have permission to view this task. Ask an owner or admin to give you access to this task.'
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
    expect(screen.getByText(/Used Codex/i)).toBeInTheDocument()
    expect(screen.queryByText(/Used codex/)).toBeNull()
    expect(screen.getAllByText('In progress').length).toBeGreaterThan(0)
    expect(screen.queryByText(/Support reference/i)).toBeNull()
    expect(screen.queryByText(/run-1234/i)).toBeNull()
    expect(screen.queryByText(/^Ref run-1234$/i)).toBeNull()
    expect(screen.queryByText(/Work method|configured worker|unknown worker|runtime/i)).toBeNull()
  })

  test('labels unknown work history tools without exposing raw tool values', async () => {
    getTaskRunsMock.mockResolvedValue([
      {
        id: 'run-tool1234',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'running',
        startedAt: '2026-04-25T06:06:00Z',
        runtimeKind: 'container',
        cliTool: 'future_tool',
      },
    ])

    render(<HistoryTab task={makeTask()} />)

    expect(await screen.findByText('Agent work history')).toBeInTheDocument()
    expect(screen.getByText(/Used a work tool that needs review/i)).toBeInTheDocument()
    expect(screen.getByText(/Support reference run-tool/i)).toBeInTheDocument()
    expect(screen.queryByText(/future_tool/i)).toBeNull()
    expect(screen.queryByText(/future tool/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  test('labels chat-only work history with AI service language', async () => {
    getTaskRunsMock.mockResolvedValue([
      {
        id: 'run-ai123456',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'completed',
        startedAt: '2026-04-25T06:06:00Z',
        finishedAt: '2026-04-25T06:07:00Z',
        runtimeKind: 'api',
      },
    ])

    render(<HistoryTab task={makeTask()} />)

    expect(await screen.findByText('Agent work history')).toBeInTheDocument()
    expect(screen.getByText(/Used an AI service/i)).toBeInTheDocument()
    expect(screen.queryByText(/model service/i)).toBeNull()
  })

  test('labels provider-backed work history without raw provider slugs', async () => {
    getTaskRunsMock.mockResolvedValue([
      {
        id: 'run-provider1',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'completed',
        startedAt: '2026-04-25T06:06:00Z',
        runtimeKind: 'api',
        providerName: 'openai_compatible',
      },
      {
        id: 'run-provider2',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'completed',
        startedAt: '2026-04-25T06:08:00Z',
        runtimeKind: 'api',
        providerName: 'future_provider',
      },
    ])

    render(<HistoryTab task={makeTask()} />)

    expect(await screen.findByText('Agent work history')).toBeInTheDocument()
    expect(screen.getByText(/Used a custom AI service/i)).toBeInTheDocument()
    expect(screen.getByText(/Used an AI service that needs review/i)).toBeInTheDocument()
    expect(screen.queryByText(/openai_compatible/i)).toBeNull()
    expect(screen.queryByText(/future_provider/i)).toBeNull()
    expect(screen.queryByText(/future provider/i)).toBeNull()
  })

  test('labels unknown work attempt states without exposing backend status values', async () => {
    getTaskRunsMock.mockResolvedValue([
      {
        id: 'run-pending123',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'pending',
        startedAt: '2026-04-25T06:06:00Z',
        runtimeKind: 'container',
      },
      {
        id: 'run-waiting123',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: 'waiting_for_result',
        startedAt: '2026-04-25T06:07:00Z',
        runtimeKind: 'container',
      },
      {
        id: 'run-missing123',
        taskId: 'task-1',
        agentId: 'agent-1',
        status: ' ',
        startedAt: '2026-04-25T06:08:00Z',
        runtimeKind: 'container',
      },
    ])

    render(<HistoryTab task={makeTask()} />)

    expect(await screen.findByText('Work attempt: Waiting to start')).toBeInTheDocument()
    expect(screen.getByText('Work attempt: Status needs review')).toBeInTheDocument()
    expect(screen.getByText('Work attempt: Status not reported')).toBeInTheDocument()
    expect(screen.queryByText(/waiting_for_result/i)).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
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

    expect(await screen.findByText('Needs review')).toBeInTheDocument()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(await screen.findAllByText(/AI service is busy/i)).toHaveLength(2)
    expect(screen.queryByText(/model service is busy/i)).toBeNull()
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

    expect(await screen.findAllByText(/Pause lower-priority work or ask an owner/i)).toHaveLength(2)
    expect(screen.queryByText(/Free capacity/i)).toBeNull()
    expect(screen.queryByText(/quota_exceeded/i)).toBeNull()
    expect(screen.queryByText(/docker socket/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
  })

  test('labels unknown task history state without exposing raw codes', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(<HistoryTab task={makeTask({ state: 'waiting_for_agent' as never })} />)

    expect(await screen.findByText('Status needs review')).toBeInTheDocument()
    expect(screen.queryByText(/waiting_for_agent/i)).toBeNull()
    expect(screen.queryByText(/waiting for agent/i)).toBeNull()
  })
})
