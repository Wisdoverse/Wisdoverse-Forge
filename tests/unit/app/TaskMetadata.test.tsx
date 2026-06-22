import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { TaskMetadata } from '@app/features/detail/TaskMetadata'

afterEach(cleanup)

const mockTask = {
  id: 'task-1',
  groupId: 'g1',
  state: 'backlog' as const,
  method: 'tasks/send',
  params: { task: 'Review onboarding copy', message: 'Make the first run easier.' },
  assignedTo: undefined,
  assignedAgentName: undefined,
  priority: 'normal' as const,
  progress: 0,
  createdAt: new Date(Date.now() - 7200000).toISOString(),
  updatedAt: new Date().toISOString(),
}

describe('TaskMetadata', () => {
  const previousBlockedStatusLabel = ['Block', 'ed'].join('')

  test('explains unassigned backlog tasks in beginner language', () => {
    render(<TaskMetadata task={mockTask} />)

    expect(screen.getByText('Not sent yet')).toBeDefined()
    expect(screen.queryByText('Backlog')).toBeNull()
    expect(screen.getByText('Agent')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Choose an agent before it can start.'
    )
    expect(screen.getByText('Needs agent')).toBeDefined()
    expect(screen.queryByText('Unassigned')).toBeNull()
  })

  test('does not call a task unassigned when only the agent id is loaded', () => {
    render(<TaskMetadata task={{ ...mockTask, assignedTo: 'agent-1' }} />)

    expect(screen.getByText('Agent name loading')).toBeDefined()
    expect(screen.queryByText('Agent details loading')).toBeNull()
    expect(screen.queryByText('Assigned agent')).toBeNull()
    expect(screen.queryByText('Unassigned')).toBeNull()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Preview the saved notes, then send it to the agent.'
    )
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toContain('publish')
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toContain('when ready')
  })

  test('labels waiting tasks without queue wording', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'queued',
          assignedAgentName: 'Build Agent',
        }}
      />
    )

    expect(screen.getByText('Waiting to start')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'waiting for the chosen agent to start. If it stays here, open Updates or choose another agent.'
    )
    expect(screen.queryByText('Queued')).toBeNull()
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toMatch(
      /queue|pick it up/i
    )
  })

  test('tells users how to recover waiting tasks that have no agent', () => {
    render(<TaskMetadata task={{ ...mockTask, state: 'queued' }} />)

    expect(screen.getByText('Waiting to start')).toBeDefined()
    expect(screen.getByText('Needs agent')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'waiting for an available agent to start. If it stays here, choose or start an agent.'
    )
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toMatch(
      /queue|pick it up/i
    )
  })

  test('describes active work check-in timing without lease wording', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'working',
          assignedAgentName: 'Build Agent',
          leaseExpiresAt: new Date(Date.now() + 60000).toISOString(),
        }}
      />
    )

    expect(screen.getByText(/Agent check-in due/i)).toBeDefined()
    expect(screen.queryByText(/Lease expires/i)).toBeNull()
  })

  test('labels retry count in beginner language', () => {
    render(<TaskMetadata task={{ ...mockTask, attempt: 2 }} />)

    expect(screen.getByText('Try 2')).toBeDefined()
    expect(screen.queryByText(/Attempt 2/i)).toBeNull()
  })

  test('does not show a broken retry label when the count is missing', () => {
    render(<TaskMetadata task={mockTask} />)

    expect(screen.queryByText(/Attempt undefined|Try undefined/i)).toBeNull()
  })

  test('turns credential blocked guidance into an account-access recovery step', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'blocked',
          assignedTo: 'agent-1',
          assignedAgentName: 'Claude-2',
          blockedReason: 'waiting_input',
          blockedHint: 'Waiting for API credentials.',
        }}
      />
    )

    expect(screen.getByText(/Waiting for account access/i)).toBeDefined()
    expect(screen.getByText(/Add or reconnect the required service access/i)).toBeDefined()
    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.queryByText(previousBlockedStatusLabel)).toBeNull()
    expect(screen.queryByText(/API credentials/i)).toBeNull()
  })

  test('keeps plain blocked guidance when it is already beginner-friendly', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'blocked',
          blockedReason: 'waiting_input',
          blockedHint: 'Waiting for your answer before continuing.',
        }}
      />
    )

    expect(screen.getByText('Waiting for your answer before continuing.')).toBeDefined()
  })

  test('explains blocked task reasons without leaking raw service details', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'blocked',
          assignedTo: 'agent-1',
          blockedReason: 'quota_exceeded',
          error: 'quota_exceeded: docker socket denied secret token abc',
        }}
      />
    )

    const guidance = screen.getByTestId('task-metadata-guidance')
    expect(guidance.textContent).toContain('Pause lower-priority work or ask an owner')
    expect(guidance.textContent).not.toMatch(/quota_exceeded|docker socket|secret token/i)
    expect(guidance.textContent).not.toContain('Free capacity')
  })

  test('explains failed task recovery without hiding the status badge', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'failed',
          error: 'Rate limit exceeded: 429 from provider',
          priority: 'high',
        }}
      />
    )

    expect(screen.getByText('Check retry steps')).toBeDefined()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getByText('High')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain('AI service is busy')
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Wait a minute, then open the task details and retry'
    )
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toContain('when ready')
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toMatch(
      /read the error|429|provider/i
    )
  })

  test('guides completed tasks with check wording', () => {
    render(<TaskMetadata task={{ ...mockTask, state: 'completed' }} />)

    expect(screen.getByText('Completed')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'The task is finished. Check the Result tab or the final answer before closing the loop.'
    )
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toContain(
      'Review the Result tab'
    )
  })

  test('labels unknown task status and priority without exposing raw codes', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'waiting_for_agent' as never,
          priority: 'future_priority' as never,
        }}
      />
    )

    expect(screen.getByText('Check task status')).toBeDefined()
    expect(screen.getByText('Check task priority')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Open Updates to check the latest task activity.'
    )
    expect(screen.queryByText(/waiting_for_agent/i)).toBeNull()
    expect(screen.queryByText(/waiting for agent/i)).toBeNull()
    expect(screen.queryByText(/future_priority/i)).toBeNull()
    expect(screen.queryByText(/future priority/i)).toBeNull()
    expect(screen.queryByText(/Open Updates to review/i)).toBeNull()
  })

  test('labels missing task status and priority with a task details step', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: ' ' as never,
          priority: ' ' as never,
        }}
      />
    )

    expect(screen.getByText('Open task details to check status')).toBeDefined()
    expect(screen.getByText('Open task details to check priority')).toBeDefined()
    expect(screen.queryByText('Refresh task status')).toBeNull()
    expect(screen.queryByText('Refresh task priority')).toBeNull()
  })

  test('explains canceled tasks with saved activity wording', () => {
    render(<TaskMetadata task={{ ...mockTask, state: 'canceled' }} />)

    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Open Updates to see the latest saved activity.'
    )
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toContain(
      'last recorded activity'
    )
  })
})
