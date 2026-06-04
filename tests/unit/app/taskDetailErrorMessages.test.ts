import { describe, expect, test } from 'vitest'
import { taskDetailErrorMessage } from '@app/features/detail/taskDetailErrorMessages'

describe('taskDetailErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expect(taskDetailErrorMessage('loadContext', new Error('401 Unauthorized'))).toBe(
      'Sign in again, then retry this task action. Code: 401.'
    )
  })

  test('describes read permission failures as view access problems', () => {
    expect(taskDetailErrorMessage('loadRuns', new Error('HTTP 403'))).toBe(
      'You do not have permission to view this task. Ask an owner or admin to update your role. Code: 403.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = taskDetailErrorMessage('loadRuns', new TypeError('Failed to fetch'))

    expect(message).toContain('Run attempts could not load')
    expect(message).toContain('The browser could not reach the server')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear next step when no agent can take the task', () => {
    expect(taskDetailErrorMessage('loadAgents', new Error('No available agent'))).toBe(
      'No agent is available for this task. Start an agent or wait for one to finish, then try again.'
    )
  })

  test('keeps short validation details after the operator instruction', () => {
    expect(
      taskDetailErrorMessage('retryTask', {
        error: 'Task is already running',
      })
    ).toBe(
      'The task was not retried. Refresh the task, then try Retry task again. Detail: Task is already running'
    )
  })
})
