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
      'Ask an owner or admin to give you access to this task, then refresh the task detail panel. You do not have permission to view this task.'
    )
  })

  test('describes update permission failures with the next step first', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('cancelTask', {
        statusCode: '403',
        serverError: 'owner role required',
      }),
      'Ask an owner or admin to let you update this task, then refresh the task detail panel and try again. You do not have permission to change this task.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = taskDetailErrorMessage('loadRuns', new TypeError('Failed to fetch'))

    expect(message).toContain('Refresh Updates before deciding whether to retry this task.')
    expect(message).toContain('If it still does not load, check your connection')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a clear next step when no agent can take the task', () => {
    expect(taskDetailErrorMessage('loadAgents', new Error('No available agent'))).toBe(
      'No agent can take this task right now. Open Agents to start or connect an agent, then refresh this task and try again.'
    )
  })

  test('uses saved item wording when the review preview cannot load', () => {
    const message = taskDetailErrorMessage('previewContext', new Error('HTTP 500'))

    expect(message).toContain('Choose an available agent, then open saved item review again.')
    expect(message).not.toMatch(new RegExp(['context', 'review'].join('\\s+'), 'i'))
  })

  test('turns service failures into a task setup recovery step', () => {
    const message = taskDetailErrorMessage('loadContext', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh the detail panel to load saved notes and work history. If it still fails, ask an owner or admin to check task setup.'
    )
    expect(message).not.toContain('run details')
    expect(message).not.toMatch(new RegExp(['task', 'context'].join('\\s+'), 'i'))
    expect(message).not.toContain('backend')
    expect(message).not.toContain('services')
  })

  test('turns cancel failures into a safe task refresh step', () => {
    const message = taskDetailErrorMessage('cancelTask', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh the task, then choose Cancel again. The task was not canceled. If it still fails, ask an owner or admin to check task setup.'
    )
    expect(message).not.toContain('HTTP 500')
  })

  test('turns needs-help failures into a safe task refresh step', () => {
    const message = taskDetailErrorMessage('blockTask', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh the task, then choose Needs help again. The task was not marked as needing help. If it still fails, ask an owner or admin to check task setup.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('blocked')
  })

  test('turns running-task details into a wait step', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', {
        error: 'Task is already running',
      }),
      'This task is already in progress. Wait for the current work to finish, then refresh the task.'
    )
  })

  test('describes context send failures without publish wording', () => {
    const message = taskDetailErrorMessage('publishTask', new Error('HTTP 500'))

    expect(message).toContain('Review the selected saved notes, then send the task again.')
    expect(message).toContain('The task was not sent with selected notes.')
    expect(message).not.toMatch(
      new RegExp(['published', 'publish', ['selected', 'context'].join('\\s+')].join('|'), 'i')
    )
    expect(message).not.toContain('HTTP 500')
  })

  test('keeps task action fallbacks next-step first', () => {
    expect(taskDetailErrorMessage('approveTask', new Error('unexpected action issue'))).toBe(
      'Check that the task is still waiting for approval, then choose Approve again. The task was not approved.'
    )
    expect(taskDetailErrorMessage('retryTask', new Error('unknown retry issue'))).toBe(
      'Refresh the task, then choose Retry task again. The task was not retried.'
    )
  })

  test('turns approval validation details into the Approve action', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('approveTask', new Error('approval state changed')),
      'Check that the task is still waiting for approval, then choose Approve again.'
    )
  })

  test('starts changed and missing task errors with the recovery step', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('HTTP 404')),
      'Refresh the board, then open the task again. This task was not found.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('HTTP 409')),
      'Refresh the detail panel, then try again. This task changed while you were working.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('HTTP 429')),
      'Wait a moment, then try again. Task actions are busy.'
    )
  })
})
