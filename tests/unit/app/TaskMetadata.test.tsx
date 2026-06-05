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
  test('explains unassigned backlog tasks in beginner language', () => {
    render(<TaskMetadata task={mockTask} />)

    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Assign an agent before it can start.'
    )
    expect(screen.getByText('Unassigned')).toBeDefined()
  })

  test('does not call a task unassigned when only the agent id is loaded', () => {
    render(<TaskMetadata task={{ ...mockTask, assignedTo: 'agent-1' }} />)

    expect(screen.getByText('Assigned agent')).toBeDefined()
    expect(screen.queryByText('Unassigned')).toBeNull()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'Preview the context and publish it when ready.'
    )
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
      'waiting for the assigned agent to start.'
    )
    expect(screen.queryByText('Queued')).toBeNull()
    expect(screen.getByTestId('task-metadata-guidance').textContent).not.toMatch(
      /queue|pick it up/i
    )
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

  test('explains failed task recovery without hiding the status badge', () => {
    render(
      <TaskMetadata
        task={{
          ...mockTask,
          state: 'failed',
          error: 'Runtime exited before producing a result.',
          priority: 'high',
        }}
      />
    )

    expect(screen.getByText('Needs review')).toBeDefined()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.getByText('High')).toBeDefined()
    expect(screen.getByTestId('task-metadata-guidance').textContent).toContain(
      'fix the cause, then retry.'
    )
  })
})
