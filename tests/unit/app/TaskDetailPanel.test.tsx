import { describe, test, expect, afterEach, vi } from 'vitest'
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
  assignedAgentName: 'Claude-2',
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
    expect(screen.getAllByText('Claude-2').length).toBeGreaterThan(0)
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
          error: 'Runtime exited before producing a result',
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
    expect(screen.getByRole('button', { name: /review skill candidates/i })).toBeDefined()
  })
})
