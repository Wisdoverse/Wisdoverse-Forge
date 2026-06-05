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
    expect(screen.getByText(/approve or update the task/i)).toBeDefined()
    expect(screen.getByText('Task story')).toBeDefined()
    expect(screen.getByText('Agent work history')).toBeDefined()
    expect(await screen.findByText('Work attempt: In Progress')).toBeDefined()
    expect(screen.getByText(/used desktop app/i)).toBeDefined()
    expect(screen.getByText(/support reference run-1234/i)).toBeDefined()
    expect(screen.getAllByText(/waiting for account access/i).length).toBeGreaterThan(0)
    expect(screen.queryByText(/waiting for api credentials/i)).toBeNull()
    expect(screen.getAllByText('Blocked').length).toBeGreaterThan(0)
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
    expect(screen.getByText('Block')).toBeDefined()
    expect(screen.getByText('Cancel')).toBeDefined()
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
    expect(screen.getByText(/resolve the blocker/i)).toBeDefined()
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

  test('approves blocked tasks waiting on human approval', async () => {
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

    await userEvent.setup().click(screen.getByRole('button', { name: /approve and continue/i }))

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

    expect(await screen.findByText(/agent work history could not load/i)).toBeDefined()
    expect(screen.getByText(/forge could not connect while loading this task/i)).toBeDefined()
    expect(screen.queryByText(/failed to fetch/i)).toBeNull()
  })

  test('surfaces reusable skill review after completed work', () => {
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

    expect(screen.getByText(/completed work can become a governed skill/i)).toBeDefined()
    expect(screen.getByTestId('task-handoff-checklist')).toBeDefined()
    expect(screen.getByText('Outcome')).toBeDefined()
    expect(screen.getByText(/solves the original request/i)).toBeDefined()
    expect(screen.getByText(/open result files or context/i)).toBeDefined()
    expect(screen.getByRole('button', { name: /review skill suggestions/i })).toBeDefined()
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
    expect(screen.getByText(/accept the result, draft reusable learning/i)).toBeDefined()
    expect(screen.getByText(/if it does not answer the brief/i)).toBeDefined()
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
        capabilities: ['review'],
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
    expect(screen.getByText('implementation, review')).toBeDefined()
    expect(screen.getByText('2 ready')).toBeDefined()

    await userEvent.setup().click(screen.getByRole('button', { name: /review agent/i }))

    expect(screen.getAllByText('Selected').length).toBe(1)
    expect(screen.getByRole('button', { name: /preview and publish/i })).toBeEnabled()
  })
})
