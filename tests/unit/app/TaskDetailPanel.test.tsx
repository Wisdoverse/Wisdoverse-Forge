import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, waitFor } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { TaskDetailPanel } from '@app/features/detail/TaskDetailPanel'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useBoardStore } from '@app/shared/model/board.store'

const orchestrationApiMock = vi.hoisted(() => ({
  updateTask: vi.fn(),
  cancelTask: vi.fn(),
  retryTask: vi.fn(),
  approveTask: vi.fn(),
  getParticipants: vi.fn(),
  getTaskRuns: vi.fn(),
  previewContext: vi.fn(),
  publishWithContext: vi.fn(),
}))

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: orchestrationApiMock,
  }
})

beforeEach(() => {
  orchestrationApiMock.updateTask.mockResolvedValue({ ok: true })
  orchestrationApiMock.cancelTask.mockResolvedValue({ ok: true })
  orchestrationApiMock.retryTask.mockResolvedValue({ ok: true, task: null })
  orchestrationApiMock.approveTask.mockResolvedValue({ ok: true, task: null })
  orchestrationApiMock.getParticipants.mockResolvedValue([])
  orchestrationApiMock.getTaskRuns.mockResolvedValue([])
  orchestrationApiMock.previewContext.mockResolvedValue(null)
  orchestrationApiMock.publishWithContext.mockResolvedValue({ ok: true, task: null })
})

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useBoardStore.getState().reset()
  vi.clearAllMocks()
})

const mockTask = {
  id: 'task-1',
  groupId: 'g1',
  state: 'working' as const,
  method: 'tasks/send',
  params: { task: 'Refactor database migration', message: 'Update the schema for v2' },
  assignedTo: 'agent-1',
  assignedAgentName: 'Agent Two',
  priority: 'high' as const,
  progress: 67,
  createdAt: new Date(Date.now() - 7200000).toISOString(),
  updatedAt: new Date().toISOString(),
}

describe('TaskDetailPanel', () => {
  const previousBlockedStatusLabel = ['Block', 'ed'].join('')
  const previousResolveCopy = new RegExp(['resolve', 'the', 'blocker'].join('\\s+'), 'i')

  test('renders task title', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getByText('Refactor database migration')).toBeDefined()
  })

  test('shows task metadata', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getAllByText('Agent Two').length).toBeGreaterThan(0)
    expect(screen.getByText('High')).toBeDefined()
    expect(screen.getAllByText('67%').length).toBeGreaterThan(0)
  })

  test('shows close button', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getByTestId('detail-close')).toBeDefined()
  })

  test('labels the task support reference instead of showing a bare task id', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)

    expect(screen.getByText('Support reference task-1')).toBeDefined()
    expect(screen.queryByText(/^task-1$/)).toBeNull()
  })

  test('tells users to refresh when the task support reference is missing', () => {
    render(<TaskDetailPanel task={{ ...mockTask, id: ' ' }} onClose={() => {}} />)

    expect(screen.getByText('Refresh task details')).toBeDefined()
    expect(screen.queryByText('Support reference not reported')).toBeNull()
  })

  test('calls onClose when close button clicked', async () => {
    const onClose = vi.fn()
    render(<TaskDetailPanel task={mockTask} onClose={onClose} />)
    await userEvent.setup().click(screen.getByTestId('detail-close'))
    expect(onClose).toHaveBeenCalledOnce()
  })

  test('shows description tab by default', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getByText('Update the schema for v2')).toBeDefined()
  })

  test('labels the saved item tab without backend wording', () => {
    useContextFeaturesStore.setState({ governance: true, preview: true, injection: true })

    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)

    expect(screen.getByRole('button', { name: /saved items/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: /^context$/i })).toBeNull()
  })

  test('summarizes agent check-ins in task updates', async () => {
    orchestrationApiMock.getTaskRuns.mockResolvedValue([
      {
        id: 'run-1234567890',
        taskId: 'task-1',
        status: 'in_progress',
        cliTool: 'desktop app',
        startedAt: new Date(Date.now() - 60000).toISOString(),
      },
    ])

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_input',
          blockedHint: 'Waiting for API credentials',
        }}
        onClose={() => {}}
      />
    )

    await userEvent.setup().click(screen.getByRole('button', { name: /updates/i }))

    expect(await screen.findByTestId('task-agent-check-in')).toBeDefined()
    expect(screen.getByText('Current status')).toBeDefined()
    expect(screen.getByTestId('task-updates-guide')).toBeDefined()
    expect(screen.getByText('What to check now')).toBeDefined()
    expect(screen.getAllByText(/needs your input/i).length).toBeGreaterThan(0)
    expect(screen.getByText(/allow it to continue or update the task/i)).toBeDefined()
    expect(screen.getByText('Task story')).toBeDefined()
    expect(screen.getByText('Agent work history')).toBeDefined()
    expect(await screen.findByText('Work attempt: In progress')).toBeDefined()
    expect(screen.getByText(/used a work tool you should check/i)).toBeDefined()
    expect(screen.getByText(/support reference run-1234/i)).toBeDefined()
    expect(screen.getAllByText(/waiting for account access/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/waiting for api credentials/i)).toBeNull()
    expect(screen.getAllByText('Needs help').length).toBeGreaterThan(0)
    expect(screen.queryByText(previousBlockedStatusLabel)).toBeNull()
  })

  test('shows completed result readiness in task updates', async () => {
    orchestrationApiMock.getTaskRuns.mockResolvedValue([])

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'completed',
          progress: 100,
          result: [{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }],
          completedAt: new Date().toISOString(),
        }}
        onClose={() => {}}
      />
    )

    await userEvent.setup().click(screen.getByRole('button', { name: /updates/i }))

    expect(await screen.findByTestId('task-agent-check-in')).toBeDefined()
    expect(screen.getByText(/agent two finished the task/i)).toBeDefined()
    expect(screen.getByText(/1 result item ready to review/i)).toBeDefined()
    expect(screen.getByText(/open results next/i)).toBeDefined()
    expect(screen.getAllByText('Completed').length).toBeGreaterThan(0)
  })

  test('has action buttons for working tasks', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getByTestId('task-live-action-guidance')).toHaveTextContent(
      'Need to pause or stop this work?'
    )
    expect(screen.getByTestId('task-live-action-guidance')).toHaveTextContent(
      'Use Needs help when the agent needs your input.'
    )
    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.queryByRole('button', { name: /^block$/i })).toBeNull()
    expect(screen.getByText('Cancel')).toBeDefined()
  })

  test('explains actions for queued tasks before stopping them', () => {
    render(<TaskDetailPanel task={{ ...mockTask, state: 'queued' }} onClose={() => {}} />)

    expect(screen.getByTestId('task-live-action-guidance')).toHaveTextContent(
      'Need to change this waiting task?'
    )
    expect(screen.getByTestId('task-live-action-guidance')).toHaveTextContent(
      'Use Cancel only if this task should not run.'
    )
  })

  test('blocks working tasks and updates the board store', async () => {
    const blockedTask = {
      ...mockTask,
      state: 'blocked' as const,
      blockedReason: 'waiting_input' as const,
    }
    orchestrationApiMock.updateTask.mockResolvedValueOnce({ ok: true, task: blockedTask })

    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)

    await userEvent.setup().click(screen.getByRole('button', { name: /^needs help$/i }))

    await waitFor(() =>
      expect(orchestrationApiMock.updateTask).toHaveBeenCalledWith('task-1', {
        state: 'blocked',
      })
    )
    expect(useBoardStore.getState().columns.blocked[0]).toMatchObject({
      id: 'task-1',
      state: 'blocked',
    })
  })

  test('explains canceling a task before stopping agent work', async () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: /^cancel$/i }))

    expect(orchestrationApiMock.cancelTask).not.toHaveBeenCalled()
    expect(
      screen.getByText(/canceling stops the current agent work/i)
    ).toBeDefined()
    expect(screen.getByRole('button', { name: /cancel task/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /keep running/i })).toBeDefined()

    await user.click(screen.getByRole('button', { name: /keep running/i }))
    expect(screen.queryByText(/canceling stops the current agent work/i)).toBeNull()

    await user.click(screen.getByRole('button', { name: /^cancel$/i }))
    await user.click(screen.getByRole('button', { name: /cancel task/i }))

    await waitFor(() => expect(orchestrationApiMock.cancelTask).toHaveBeenCalledWith('task-1'))
  })

  test('shows beginner guidance when cancel fails', async () => {
    orchestrationApiMock.cancelTask.mockRejectedValueOnce(new Error('HTTP 500'))

    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /^cancel$/i }))
    await user.click(screen.getByRole('button', { name: /cancel task/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Refresh the task, then choose Cancel again.')
    expect(alert).toHaveTextContent('The task was not canceled.')
    expect(alert).not.toHaveTextContent('HTTP 500')
  })

  test('shows next action guidance for blocked tasks', () => {
    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_owner',
          blockedHint: 'Waiting for deployment approval',
        }}
        onClose={() => {}}
      />
    )

    expect(screen.getByTestId('task-next-action')).toBeDefined()
    expect(screen.getByText(/provide what is missing/i)).toBeDefined()
    expect(screen.queryByText(previousResolveCopy)).toBeNull()
    expect(screen.getAllByText(/waiting for deployment approval/i).length).toBeGreaterThan(0)
  })

  test('retries failed tasks and updates the board store', async () => {
    const retriedTask = { ...mockTask, state: 'queued' as const, progress: 0, error: undefined }
    orchestrationApiMock.retryTask.mockResolvedValue({ ok: true, task: retriedTask })

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'failed',
          error: 'Worker stopped before producing a result',
        }}
        onClose={() => {}}
      />
    )

    expect(screen.getByTestId('task-recovery-actions')).toBeDefined()
    expect(screen.getByTestId('task-recovery-guidance')).toHaveTextContent(
      'Try the task again when the request is still useful'
    )
    expect(screen.getByTestId('task-recovery-guidance')).toHaveTextContent(
      'goes back to the queue'
    )
    await userEvent.setup().click(screen.getByRole('button', { name: /retry task/i }))

    await waitFor(() => expect(orchestrationApiMock.retryTask).toHaveBeenCalledWith('task-1'))
    expect(useBoardStore.getState().columns.queued[0]).toMatchObject({
      id: 'task-1',
      state: 'queued',
    })
  })

  test('summarizes failed task errors without raw service details', () => {
    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'failed',
          error: 'Rate limit exceeded: 429 from provider',
        }}
        onClose={() => {}}
      />
    )

    const preview = screen.getByTestId('task-detail-failure-preview')
    expect(preview.textContent).toContain('AI service is busy')
    expect(preview.textContent).not.toContain('429')
    expect(preview.textContent).not.toContain('provider')
    expect(preview.textContent).not.toContain('model service is busy')
  })

  test('shows beginner guidance when retry fails', async () => {
    orchestrationApiMock.retryTask.mockRejectedValueOnce(new Error('409 conflict'))

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'failed',
          error: 'Worker stopped before producing a result',
        }}
        onClose={() => {}}
      />
    )

    await userEvent.setup().click(screen.getByRole('button', { name: /retry task/i }))

    expect(await screen.findByText(/this task changed while you were working/i)).toBeDefined()
    expect(screen.queryByText(/409 conflict/i)).toBeNull()
  })

  test('allows blocked tasks waiting on a human decision to continue', async () => {
    const approvedTask = { ...mockTask, state: 'queued' as const, blockedReason: undefined }
    orchestrationApiMock.approveTask.mockResolvedValue({ ok: true, task: approvedTask })

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_approval',
          blockedHint: 'Waiting for release owner approval',
        }}
        onClose={() => {}}
      />
    )

    expect(screen.getByTestId('task-recovery-guidance')).toHaveTextContent(
      'Let the task continue when it has what it needs'
    )
    expect(screen.getByTestId('task-recovery-guidance')).toHaveTextContent(
      'return the task to the queue'
    )
    await userEvent.setup().click(screen.getByRole('button', { name: /allow and continue/i }))

    await waitFor(() => expect(orchestrationApiMock.approveTask).toHaveBeenCalledWith('task-1'))
    expect(useBoardStore.getState().columns.queued[0]).toMatchObject({
      id: 'task-1',
      state: 'queued',
    })
  })

  test('shows beginner guidance when agent work history fails to load', async () => {
    orchestrationApiMock.getTaskRuns.mockRejectedValueOnce(new TypeError('Failed to fetch'))

    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)

    await userEvent.setup().click(screen.getByRole('button', { name: /updates/i }))

    expect(
      await screen.findByText(/refresh updates before deciding whether to retry this task/i)
    ).toBeDefined()
    expect(screen.getByText(/check your connection and refresh the page/i)).toBeDefined()
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
  })

  test('surfaces saved instruction review after completed work', () => {
    useContextFeaturesStore.setState({ governance: true, preview: true, injection: true })
    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'completed',
          progress: 100,
          result: [{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }],
          completedAt: new Date().toISOString(),
        }}
        onClose={() => {}}
      />
    )

    expect(screen.getAllByText(/save the repeatable steps/i).length).toBeGreaterThanOrEqual(2)
    expect(screen.getByTestId('task-handoff-checklist')).toBeDefined()
    expect(screen.getByText('Outcome')).toBeDefined()
    expect(screen.getByText(/solves the original request/i)).toBeDefined()
    expect(screen.getByText(/open result files or what the agent used/i)).toBeDefined()
    expect(screen.queryByText(/open result files or context/i)).toBeNull()
    expect(screen.getByText(/future tasks should reuse them/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /review save ideas/i })).toBeDefined()
  })

  test('guides beginner review on the result tab', async () => {
    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'completed',
          progress: 100,
          result: [
            {
              name: 'summary.md',
              mimeType: 'text/markdown',
              data: 'Updated migration plan and validation notes.',
            },
          ],
          completedAt: new Date().toISOString(),
        }}
        onClose={() => {}}
      />
    )

    await userEvent.setup().click(screen.getByRole('button', { name: /^result$/i }))

    expect(screen.getByTestId('task-result-review-guide')).toBeDefined()
    expect(screen.getByText(/review the result before closing/i)).toBeDefined()
    expect(screen.getByText(/compare with the brief/i)).toBeDefined()
    expect(screen.getByText(/1 result file attached for review/i)).toBeDefined()
    expect(screen.getByText(/accept the result, save repeatable steps/i)).toBeDefined()
    expect(screen.getByText('Text result')).toBeDefined()
    expect(screen.queryByText('text/markdown')).toBeNull()
    const previousResultReuseCopy = new RegExp(['draft', 'saved guidance'].join('.*'), 'i')
    const previousAddContextCopy = new RegExp(['add', 'context'].join('\\s+'), 'i')
    expect(screen.queryByText(previousResultReuseCopy)).toBeNull()
    expect(screen.getByText(/if it does not answer the brief/i)).toBeDefined()
    expect(screen.getByText(/review saved notes and instructions/i)).toBeDefined()
    expect(screen.queryByText(previousAddContextCopy)).toBeNull()
  })

  test('uses beginner-friendly names for text result files', async () => {
    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'completed',
          progress: 100,
          result: {
            stdout: 'Reviewed the setup and produced final notes.',
          },
          completedAt: new Date().toISOString(),
        }}
        onClose={() => {}}
      />
    )

    await userEvent.setup().click(screen.getByRole('button', { name: /^result$/i }))

    expect(screen.getByText('text-result.txt')).toBeDefined()
    expect(screen.getByText('Text result')).toBeDefined()
    expect(screen.queryByText('text/plain')).toBeNull()
    expect(screen.queryByText('stdout.txt')).toBeNull()
  })

  test('shows available agents as selectable handoff cards', async () => {
    useContextFeaturesStore.setState({ governance: true, preview: true, injection: true })
    orchestrationApiMock.getParticipants.mockResolvedValue([
      {
        id: 'participant-1',
        agentId: 'agent-1',
        name: 'Builder Agent',
        status: 'available',
        capabilities: ['implementation', 'review'],
      },
      {
        id: 'participant-2',
        agentId: 'agent-2',
        name: 'Review Agent',
        status: 'available',
        capabilities: ['code-review'],
      },
      {
        id: 'participant-3',
        agentId: 'agent-3',
        name: 'Ready Agent',
        status: 'available',
        capabilities: [],
      },
    ])

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'backlog',
          assignedTo: undefined,
          assignedAgentName: undefined,
        }}
        onClose={() => {}}
      />
    )

    expect(await screen.findByText('Available agents')).toBeDefined()
    expect(screen.getByText('Builder Agent')).toBeDefined()
    expect(screen.getByText('Review Agent')).toBeDefined()
    expect(screen.getByText('Ready Agent')).toBeDefined()
    expect(screen.getByText('Can build the task and review the result')).toBeDefined()
    expect(screen.getByText('Can help with code review')).toBeDefined()
    expect(screen.queryByText('implementation, review')).toBeNull()
    expect(screen.queryByText('code-review')).toBeNull()
    expect(screen.getByText('Ready to take this task')).toBeDefined()
    expect(screen.queryByText('Ready for assignment')).toBeNull()
    expect(screen.getByText('3 ready')).toBeDefined()

    await userEvent.setup().click(screen.getByRole('button', { name: /review agent/i }))

    expect(screen.getAllByText('Selected').length).toBe(1)
    expect(screen.getByRole('button', { name: /preview and send/i })).toBeEnabled()
    expect(screen.queryByRole('button', { name: /preview and publish/i })).toBeNull()
  })

  test('guides users to agent setup when no agent can take the task', async () => {
    useContextFeaturesStore.setState({ governance: true, preview: true, injection: true })
    orchestrationApiMock.getParticipants.mockResolvedValue([])

    render(
      <TaskDetailPanel
        task={{
          ...mockTask,
          state: 'backlog',
          assignedTo: undefined,
          assignedAgentName: undefined,
        }}
        onClose={() => {}}
      />
    )

    expect(await screen.findByText('No agent can take this task right now')).toBeDefined()
    expect(
      screen.getByText(/open agents to start or connect an agent, then return here and refresh/i)
    ).toBeDefined()
    expect(screen.queryByText('No available agent can take this task right now.')).toBeNull()

    const sendButton = screen.getByRole('button', {
      name: /choose an available agent before sending/i,
    })
    expect(sendButton).toBeDisabled()
    expect(sendButton).toHaveAttribute('title', 'Choose an available agent before sending')
  })
})
