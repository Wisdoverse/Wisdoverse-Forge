import { describe, test, expect, afterEach, vi } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { TaskDetailPanel } from '@app/features/detail/TaskDetailPanel'

afterEach(cleanup)

const mockTask = {
  id: 'task-1', groupId: 'g1', state: 'working' as const, method: 'tasks/send',
  params: { task: 'Refactor database migration', message: 'Update the schema for v2' },
  assignedTo: 'agent-1', assignedAgentName: 'Claude-2', priority: 'high' as const,
  progress: 67, createdAt: new Date(Date.now() - 7200000).toISOString(), updatedAt: new Date().toISOString(),
}

describe('TaskDetailPanel', () => {
  test('renders task title', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getByText('Refactor database migration')).toBeDefined()
  })

  test('shows task metadata', () => {
    render(<TaskDetailPanel task={mockTask} onClose={() => {}} />)
    expect(screen.getByText('Claude-2')).toBeDefined()
    expect(screen.getByText('High')).toBeDefined()
    expect(screen.getByText('67%')).toBeDefined()
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
})
