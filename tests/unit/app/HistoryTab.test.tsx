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

  test('tells users to choose an agent before starting unassigned backlog tasks', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'backlog',
          assignedAgentName: undefined,
          assignedTo: undefined,
          progress: 0,
        })}
      />
    )

    expect(await screen.findByText('Choose an agent to start this task')).toBeInTheDocument()
    expect(screen.getByText('Choose an available agent before this task can start.')).toBeDefined()
    expect(screen.getByText('Choose an agent first, then start the task.')).toBeDefined()
    expect(screen.queryByText('No agent assigned yet')).toBeNull()
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
    expect(screen.getByText('Work attempt: Refresh task status')).toBeInTheDocument()
    expect(screen.queryByText('Work attempt: Status not reported')).toBeNull()
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
    expect(
      screen.getByText(/check work history below, then choose another agent/i)
    ).toBeInTheDocument()
    expect(
      screen.getByText(/If this stays here, check the work history below/i)
    ).toBeInTheDocument()
    expect(screen.queryByText('Queued')).toBeNull()
    expect(screen.queryByText(/Nothing is needed yet/i)).toBeNull()
    expect(screen.queryByText(/Check back/i)).toBeNull()
  })

  test('tells users how to start waiting history when no agent is assigned', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'queued',
          assignedAgentName: undefined,
          assignedTo: undefined,
          progress: 0,
        })}
      />
    )

    expect(await screen.findByText('Waiting for an available agent')).toBeInTheDocument()
    expect(screen.getByText('Needs agent')).toBeInTheDocument()
    expect(screen.getByText(/Choose or start an agent so this task/i)).toBeInTheDocument()
    expect(
      screen.getByText(/Choose or start an agent before expecting work history/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/The agent is waiting/i)).toBeNull()
    expect(screen.queryByText(/Nothing is needed yet/i)).toBeNull()
  })

  test('does not call a waiting task unassigned while agent details load', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'queued',
          assignedAgentName: undefined,
          assignedTo: 'agent-1',
          progress: 0,
        })}
      />
    )

    expect(await screen.findByText('The chosen agent is waiting to start')).toBeInTheDocument()
    expect(screen.getByText('Agent details loading')).toBeInTheDocument()
    expect(screen.queryByText('Needs agent')).toBeNull()
  })

  test('guides completed tasks toward result review without saved guidance jargon', async () => {
    getTaskRunsMock.mockResolvedValue([])

    render(
      <HistoryTab
        task={makeTask({
          state: 'completed',
          progress: 100,
          completedAt: '2026-04-25T06:20:00Z',
        })}
      />
    )

    expect(await screen.findByText('Build Agent finished the task')).toBeInTheDocument()
    expect(screen.getByText(/Review the outcome, then save repeatable steps/i)).toBeInTheDocument()
    expect(screen.getByText(/Open Results next. Check the answer/i)).toBeInTheDocument()
    expect(screen.queryByText(/saved guidance/i)).toBeNull()
    expect(screen.queryByText(/Confirm the answer matches the brief/i)).toBeNull()
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
