import { describe, test, expect, afterEach, vi } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { TaskDetailPanel } from '@app/features/detail/TaskDetailPanel'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
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
