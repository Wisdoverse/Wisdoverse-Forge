import { describe, test, expect, afterEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react'
import '@app/i18n'
import { TaskCard } from '@app/features/board/TaskCard'

const trackProductEventMock = vi.hoisted(() => vi.fn().mockResolvedValue(undefined))

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    trackProductEvent: trackProductEventMock,
  }
})

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

    expect(screen.getByText('Chosen agent')).toBeDefined()
    expect(screen.queryByText('Assigned agent')).toBeNull()
    expect(screen.queryByText('No assignee')).toBeNull()
  })

  test('labels tasks without any agent as needing an agent', () => {
    render(<TaskCard task={{ ...mockTask, assignedTo: undefined, assignedAgentName: undefined }} />)

    expect(screen.getByText('Needs agent')).toBeDefined()
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

  test('shows a queued wait estimate with a why hint', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'queued',
          progress: 0,
          waitEstimate: { position: 2, typicalSeconds: 90, estimatedSeconds: 180 },
        }}
      />
    )
    const estimate = screen.getByTestId('task-wait-estimate-task-1')
    expect(estimate.textContent).toContain('Starts in ~3 min')
    expect(estimate.getAttribute('title')).toContain('Position 2 in the queue')
    expect(estimate.getAttribute('title')).toContain('change the agent or priority')
  })

  test('labels a wait estimate without history as a rough guess', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'queued',
          progress: 0,
          waitEstimate: { position: 1, typicalSeconds: 0, estimatedSeconds: 300 },
        }}
      />
    )
    const estimate = screen.getByTestId('task-wait-estimate-task-1')
    expect(estimate.textContent).toContain('Starts in ~5 min')
    expect(estimate.getAttribute('title')).toContain('rough guess')
  })

  test('does not show an estimate for terminal or working tasks', () => {
    render(<TaskCard task={{ ...mockTask, state: 'working', waitEstimate: { position: 1, typicalSeconds: 90, estimatedSeconds: 90 } }} />)
    expect(screen.queryByTestId('task-wait-estimate-task-1')).toBeNull()
  })

  test('explains a context overflow failure on the card and records it once', async () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'failed',
          progress: 0,
          error: 'invalid_request_error: prompt is too long: 201k tokens',
        }}
      />
    )
    expect(screen.getByTestId('task-error-preview').textContent).toContain('Ran out of context window')
    await waitFor(() => expect(trackProductEventMock).toHaveBeenCalledTimes(1))
    expect(trackProductEventMock).toHaveBeenCalledWith('context_overflow_failure', { taskId: 'task-1' })

    // Re-rendering the same card must not duplicate the best-effort event.
    cleanup()
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'failed',
          progress: 0,
          error: 'context length exceeded for the model',
        }}
      />
    )
    expect(trackProductEventMock).toHaveBeenCalledTimes(1)
  })

  test('labels a retried failure with its attempt number', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'failed',
          progress: 0,
          attempt: 2,
          error: 'timed out mid-step',
        }}
      />
    )
    const preview = screen.getByTestId('task-error-preview')
    expect(preview.textContent).toContain('took too long')
    expect(preview.textContent).toContain('attempt 2')
    expect(trackProductEventMock).not.toHaveBeenCalledWith('context_overflow_failure', expect.anything())
  })

  test('does not add an attempt note on the first try', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'failed',
          progress: 0,
          attempt: 1,
          error: 'timed out mid-step',
        }}
      />
    )
    expect(screen.getByTestId('task-error-preview').textContent).not.toContain('attempt')
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

    expect(screen.getByText('Open task details to read this priority')).toBeDefined()
    expect(screen.getByText('Open task details to read this status')).toBeDefined()
    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open task details to check the current status before taking action.'
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
    expect(badge.getAttribute('aria-label')).toBe('2 saved notes added, 1 skill added')
    expect(badge.getAttribute('aria-label')).not.toContain('saved instruction')
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
      'Choose an agent, then preview and send.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('publish')
  })

  test('shows a direct send next step after an agent is selected', () => {
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

    expect(screen.getByTestId('task-next-step').textContent).toBe('Check context items, then send.')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('when ready')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('saved items')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('publish')
  })

  test('does not send title-only backlog tasks toward sending', () => {
    const onPublish = vi.fn()
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'backlog',
          params: { ...mockTask.params, message: '' },
          progress: 0,
        }}
        onPublish={onPublish}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open this card and add details before sending.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('Review saved items')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('publishing')
  })

  test('asks for details before agent choice on title-only backlog tasks', () => {
    const onPublish = vi.fn()
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'backlog',
          assignedTo: undefined,
          assignedAgentName: undefined,
          params: { ...mockTask.params, message: '   ' },
          progress: 0,
        }}
        onPublish={onPublish}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open this card, add details, then choose an agent.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('preview and publish')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('publish')
  })

  test('tells operators how to finish a saved task card before sending', () => {
    render(<TaskCard task={{ ...mockTask, state: 'backlog', progress: 0 }} />)

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open this card, add details, then send it to an agent.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('Open details')
  })

  test('shows what to do when an assigned task is still waiting to start', () => {
    render(<TaskCard task={{ ...mockTask, state: 'queued', progress: 0 }} />)

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Waiting for the chosen agent to start. If it stays here, open task details or choose another agent.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('assigned agent')
  })

  test('shows how to recover a waiting task that has no agent yet', () => {
    render(
      <TaskCard
        task={{
          ...mockTask,
          state: 'queued',
          assignedTo: undefined,
          assignedAgentName: undefined,
          progress: 0,
        }}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Waiting for an agent to start. If it stays here, choose or start an agent.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('pick this up')
  })

  test('shows a recovery next step for failed tasks', () => {
    render(
      <TaskCard
        task={{ ...mockTask, state: 'failed', error: 'Rate limit exceeded: 429 from provider' }}
      />
    )

    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open task details, read the recovery note, then retry.'
    )
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('error')
    expect(screen.getByTestId('task-next-step').textContent).not.toContain('failure')
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
    expect(preview.textContent).toContain('Wait a minute, then open the task details and try again')
    expect(preview.textContent).not.toContain('when ready')
    expect(preview.textContent).not.toContain('429')
    expect(preview.textContent).not.toContain('provider')
    expect(preview.textContent).not.toContain('model service is busy')
    expect(preview.getAttribute('title')).toContain('AI service is busy')
    expect(preview.getAttribute('title')).toContain(
      'Wait a minute, then open the task details and try again'
    )
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
    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open task details, check result files, then save repeatable steps or create a follow-up task.'
    )
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

  test('opens send preview for backlog tasks', () => {
    const onPublish = vi.fn()
    render(<TaskCard task={{ ...mockTask, state: 'backlog' }} onPublish={onPublish} />)

    fireEvent.click(
      screen.getByRole('button', { name: 'Preview and send Refactor database migration' })
    )

    expect(onPublish).toHaveBeenCalledWith(expect.objectContaining({ id: 'task-1' }))
  })

  test('does not show result count when completed task has no results', () => {
    render(<TaskCard task={{ ...mockTask, state: 'completed', result: [] }} />)
    expect(screen.queryByTestId('task-result-count')).toBeNull()
    expect(screen.getByTestId('task-next-step').textContent).toBe(
      'Open task details, check the final answer, then save repeatable steps or create a follow-up task.'
    )
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
