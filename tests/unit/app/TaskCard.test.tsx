import { describe, test, expect, afterEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent } from '@testing-library/react'
import { TaskCard } from '@app/features/board/TaskCard'

afterEach(cleanup)

const mockTask = {
  id: 'task-1',
  groupId: 'g1',
  state: 'working' as const,
  method: 'tasks/send',
  params: { task: 'Refactor database migration', message: 'Detailed description here' },
  assignedTo: 'agent-1',
  assignedAgentName: 'Claude-2',
  priority: 'high' as const,
  progress: 67,
  createdAt: new Date(Date.now() - 7200000).toISOString(),
  updatedAt: new Date().toISOString(),
}

describe('TaskCard', () => {
  test('renders task title', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByText('Refactor database migration')).toBeDefined()
  })

  test('shows agent name when assigned', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByText('Claude-2')).toBeDefined()
  })

  test('shows progress bar for working state', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByTestId('progress-bar')).toBeDefined()
  })

  test('shows priority badge', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByText('High')).toBeDefined()
  })

  test('shows context badge when applied memory or skill counts are present', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          contextCounts: { appliedMemories: 2, appliedSkills: 1, total: 3 },
        }}
      />
    )

    const badge = screen.getByTestId('task-context-badge')
    expect(badge).toBeDefined()
    expect(badge.textContent).toContain('2')
    expect(badge.textContent).toContain('1')
    expect(badge.getAttribute('aria-label')).toBe('2 applied memories, 1 applied skill')
  })

  test('hides context badge when no context has been applied', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          contextCounts: { appliedMemories: 0, appliedSkills: 0, total: 0 },
        }}
      />
    )
    expect(screen.queryByTestId('task-context-badge')).toBeNull()
  })

  test('does not show progress for backlog tasks', () => {
    render(<TaskCard task={{ ...mockTask, state: 'backlog', progress: 0 }} />)
    expect(screen.queryByTestId('progress-bar')).toBeNull()
  })

  test('shows error preview for failed tasks', () => {
    render(
      <TaskCard
        task={{ ...mockTask, state: 'failed', error: 'Rate limit exceeded: 429 from provider' }}
      />
    )
    const preview = screen.getByTestId('task-error-preview')
    expect(preview).toBeDefined()
    expect(preview.textContent).toContain('Rate limit exceeded')
  })

  test('does not show error preview when state is not failed', () => {
    render(<TaskCard task={{ ...mockTask, error: 'ignored' }} />)
    expect(screen.queryByTestId('task-error-preview')).toBeNull()
  })

  test('does not show error preview when failed state has no error field', () => {
    render(<TaskCard task={{ ...mockTask, state: 'failed' }} />)
    expect(screen.queryByTestId('task-error-preview')).toBeNull()
  })

  test('shows result count for completed tasks with attachments', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'completed',
          result: [
            { name: 'a.txt', mimeType: 'text/plain', data: '' },
            { name: 'b.md', mimeType: 'text/markdown', data: '' },
          ],
        }}
      />
    )
    const count = screen.getByTestId('task-result-count')
    expect(count).toBeDefined()
    expect(count.textContent).toBe('2 files')
  })

  test('shows stdout result count for real sidecar completions', () => {
    render(
      <TaskCard task={{ ...mockTask, state: 'completed', result: { stdout: 'real output' } }} />
    )

    const count = screen.getByTestId('task-result-count')
    expect(count.textContent).toBe('1 file')
    expect(count.getAttribute('title')).toBe('1 attachment')
  })

  test('uses singular "file" for a single result', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'completed',
          result: [{ name: 'only.txt', mimeType: 'text/plain', data: '' }],
        }}
      />
    )
    expect(screen.getByTestId('task-result-count').textContent).toBe('1 file')
  })

  test('does not show result count when task is not completed', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          result: [{ name: 'a.txt', mimeType: 'text/plain', data: '' }],
        }}
      />
    )
    expect(screen.queryByTestId('task-result-count')).toBeNull()
  })

  test('opens publish preview for backlog tasks', () => {
    const onPublish = vi.fn()
    render(<TaskCard task={{ ...mockTask, state: 'backlog' }} onPublish={onPublish} />)

    fireEvent.click(screen.getByRole('button', { name: 'Publish Refactor database migration' }))

    expect(onPublish).toHaveBeenCalledWith(expect.objectContaining({ id: 'task-1' }))
  })

  test('does not show result count when completed task has no results', () => {
    render(<TaskCard task={{ ...mockTask, state: 'completed', result: [] }} />)
    expect(screen.queryByTestId('task-result-count')).toBeNull()
  })

  test('activates from a short pointer tap without double firing the follow-up click', () => {
    const onClick = vi.fn()
    render(<TaskCard task={mockTask} onClick={onClick} />)

    const card = screen.getByTestId('task-card-task-1')
    fireEvent.pointerDown(card, { button: 0, clientX: 24, clientY: 32 })
    fireEvent.pointerUp(card, { button: 0, clientX: 26, clientY: 33 })
    fireEvent.click(card)

    expect(onClick).toHaveBeenCalledTimes(1)
  })

  test('activates from a short mouse press before drag handlers can suppress click', () => {
    const onClick = vi.fn()
    render(<TaskCard task={mockTask} onClick={onClick} />)

    const card = screen.getByTestId('task-card-task-1')
    fireEvent.mouseDown(card, { button: 0, clientX: 24, clientY: 32 })
    fireEvent.mouseUp(card, { button: 0, clientX: 25, clientY: 33 })
    fireEvent.click(card)

    expect(onClick).toHaveBeenCalledTimes(1)
  })

  test('does not treat drag movement as a tap activation', () => {
    const onClick = vi.fn()
    render(<TaskCard task={mockTask} onClick={onClick} />)

    const card = screen.getByTestId('task-card-task-1')
    fireEvent.pointerDown(card, { button: 0, clientX: 24, clientY: 32 })
    fireEvent.pointerUp(card, { button: 0, clientX: 48, clientY: 60 })

    expect(onClick).not.toHaveBeenCalled()
  })
})
