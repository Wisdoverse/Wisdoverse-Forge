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

  test('shows readable status and update time instead of the internal task id', () => {
    render(<TaskCard task={mockTask} />)

    expect(screen.getByText('Working')).toBeDefined()
    expect(screen.getByText(/Updated (just now|\d+[mhd] ago)/)).toBeDefined()
    expect(screen.queryByText('task-1')).toBeNull()
  })

  test('shows agent name when assigned', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByText('Claude-2')).toBeDefined()
  })

  test('does not call a task unassigned when only the agent id is loaded', () => {
    render(<TaskCard task={{ ...mockTask, assignedAgentName: undefined }} />)

    expect(screen.getByText('Assigned agent')).toBeDefined()
    expect(screen.queryByText('No assignee')).toBeNull()
  })

  test('shows progress bar for working state', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByTestId('progress-bar')).toBeDefined()
  })

  test('shows priority badge', () => {
    render(<TaskCard task={mockTask} />)
    expect(screen.getByText('High')).toBeDefined()
  })

  test('labels unknown status and priority without exposing raw codes', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'waiting_for_agent' as never,
          priority: 'future_priority' as never,
          progress: 0,
        }}
      />
    )

    expect(screen.getByText('Priority needs review')).toBeDefined()
    expect(screen.getByText('Status needs review')).toBeDefined()
    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open details to check the current status before taking action.'
    )
    expect(screen.queryByText(/waiting_for_agent/i)).toBeNull()
    expect(screen.queryByText(/waiting for agent/i)).toBeNull()
    expect(screen.queryByText(/future_priority/i)).toBeNull()
    expect(screen.queryByText(/future priority/i)).toBeNull()
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
    expect(badge.getAttribute('aria-label')).toBe('2 saved notes added, 1 saved instruction added')
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

  test('shows a beginner next step for unassigned backlog tasks', () => {
    const onPublish = vi.fn()
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'backlog',
          assignedTo: undefined,
          assignedAgentName: undefined,
          progress: 0,
        }}
        onPublish={onPublish}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Choose an agent, then preview and publish.'
    )
  })

  test('shows a direct publish next step after an agent is selected', () => {
    const onPublish = vi.fn()
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'backlog',
          progress: 0,
        }}
        onPublish={onPublish}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Review saved items, then publish.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('when ready')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('context')
  })

  test('shows a recovery next step for failed tasks', () => {
    render(
      <TaskCard
        task={{ ...mockTask, state: 'failed', error: 'Rate limit exceeded: 429 from provider' }}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open details, fix the error, then retry.'
    )
  })

  test('does not duplicate server-provided blocked guidance', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_agent',
          blockedHint: 'Waiting for an available agent.',
        }}
      />
    )

    expect(screen.getByTestId('task-blocked-hint-task-1')).toBeDefined()
    expect(screen.queryByTestId('task-next-step')).toBeNull()
  })

  test('shows beginner-safe blocked guidance without sensitive raw hints', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_input',
          blockedHint: 'Missing token secret for git provider.',
        }}
      />
    )

    const hint = screen.getByTestId('task-blocked-hint-task-1')
    expect(hint.textContent).toContain('Waiting for account access')
    expect(hint.textContent).not.toContain('token')
    expect(hint.textContent).not.toContain('secret')
    expect(hint.getAttribute('title')).toContain('Waiting for account access')
    expect(hint.getAttribute('title')).not.toContain('token')
    expect(hint.getAttribute('title')).not.toContain('secret')
    expect(screen.queryByTestId('task-next-step')).toBeNull()
  })

  test('hides beginner next steps in compact mode', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'backlog',
          assignedTo: undefined,
          assignedAgentName: undefined,
          progress: 0,
        }}
        displayMode="compact"
      />
    )

    expect(screen.queryByTestId('task-next-step')).toBeNull()
  })

  test('shows error preview for failed tasks', () => {
    render(
      <TaskCard
        task={{ ...mockTask, state: 'failed', error: 'Rate limit exceeded: 429 from provider' }}
      />
    )
    const preview = screen.getByTestId('task-error-preview')
    expect(preview).toBeDefined()
    expect(preview.textContent).toContain('AI service is busy')
    expect(preview.textContent).toContain('Wait a minute, then open details and retry')
    expect(preview.textContent).not.toContain('when ready')
    expect(preview.textContent).not.toContain('429')
    expect(preview.textContent).not.toContain('provider')
    expect(preview.textContent).not.toContain('model service is busy')
    expect(preview.getAttribute('title')).toContain('AI service is busy')
    expect(preview.getAttribute('title')).toContain('Wait a minute, then open details and retry')
    expect(preview.getAttribute('title')).not.toContain('when ready')
    expect(preview.getAttribute('title')).not.toContain('429')
    expect(preview.getAttribute('title')).not.toContain('provider')
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

  test('shows stdout result count for real connection-tool completions', () => {
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
