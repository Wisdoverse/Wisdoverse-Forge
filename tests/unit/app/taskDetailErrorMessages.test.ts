import { describe, expect, test } from 'vitest'
import { taskDetailErrorMessage } from '@app/features/detail/taskDetailErrorMessages'

describe('taskDetailErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Detail:')
  }

  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('loadContext', new Error('401 Unauthorized')),
      'Sign in again, then retry this task action.'
    )
  })

  test('describes read permission failures as view access problems', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('loadRuns', new Error('HTTP 403')),
      'You do not have permission to view this task. Ask an owner or admin to give you access to this task.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = taskDetailErrorMessage('loadRuns', new TypeError('Failed to fetch'))

    expect(message).toContain('Agent work history could not load')
    expect(message).toContain('Forge could not connect while loading this task')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a clear next step when no agent can take the task', () => {
    expect(taskDetailErrorMessage('loadAgents', new Error('No available agent'))).toBe(
      'No agent is available for this task. Start an agent or wait for one to finish, then try again.'
    )
  })

  test('turns service failures into a task setup recovery step', () => {
    const message = taskDetailErrorMessage('loadContext', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Task context could not load. Refresh the detail panel, then try again. Forge could not load task details right now. Refresh the task, then try again. If it still fails, ask an owner or admin to check task setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('services')
  })

  test('turns running-task details into a wait step', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', {
        error: 'Task is already running',
      }),
      'This task is already running. Wait for the current run to finish, then refresh the task.'
    )
  })
})
