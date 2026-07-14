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
      'Sign in again, then open this task again from the Tasks page to load saved notes and work history.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('401 Unauthorized')),
      'Sign in again, then open this task again from the Tasks page, then choose Retry task again. The task was not retried.'
    )
  })

  test('describes read permission failures as view access problems', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('loadRuns', new Error('HTTP 403')),
      'Ask an owner or admin to give you access to this task, then open it again from the Tasks page. You do not have permission to view this task.'
    )
  })

  test('describes update permission failures with the next step first', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('cancelTask', {
        statusCode: '403',
        serverError: 'owner role required',
      }),
      'Ask an owner or admin to let you update this task, then open this task again from the Tasks page, then choose Cancel again. The task was not canceled. You do not have permission to change this task.'
    )
  })

  test('describes role-required read failures as view access problems', () => {
    const message = taskDetailErrorMessage('loadRuns', 'owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to this task, then open it again from the Tasks page. You do not have permission to view this task.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = taskDetailErrorMessage('loadRuns', new TypeError('Failed to fetch'))

    expect(message).toContain(
      'Open Updates for this task again before deciding whether to retry this task.'
    )
    expect(message).toContain('If it still does not load, check your connection')
    expect(message).toContain('open this task again from the Tasks page')
    expect(message).not.toContain('refresh the page')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('describes update network failures with the visible task action path', () => {
    const message = taskDetailErrorMessage('cancelTask', new TypeError('Network error'))

    expect(message).toContain('Open this task again from the Tasks page, then choose Cancel again.')
    expect(message).toContain('check your connection before choosing Cancel again')
    expect(message).not.toContain('choose the action again')
    expect(message).not.toContain('choosing the action again')
    expect(message).not.toContain('try again. Task actions are busy')
    expect(message).not.toContain('check your connection and try again')
  })

  test('gives a clear next step when no agent can take the task', () => {
    expect(taskDetailErrorMessage('loadAgents', new Error('No available agent'))).toBe(
      'No agent can take this task right now. Open Agents to start or connect an agent, then open this task again from the Tasks page.'
    )
  })

  test('uses saved items wording when the saved item check cannot load', () => {
    const message = taskDetailErrorMessage('previewContext', new Error('HTTP 500'))

    expect(message).toContain('Choose an agent, then check saved items again.')
    expect(message).not.toContain('available agent')
    expect(message).not.toMatch(new RegExp(['context', 'review'].join('\\s+'), 'i'))
  })

  test('turns service failures into a task setup recovery step', () => {
    const message = taskDetailErrorMessage('loadContext', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Open this task again from the Tasks page to load saved notes and work history. If it still fails, ask an owner or admin to check task details access.'
    )
    expect(message).not.toContain('run details')
    expect(message).not.toMatch(new RegExp(['task', 'context'].join('\\s+'), 'i'))
    expect(message).not.toContain('backend')
    expect(message).not.toContain('services')
  })

  test('keeps unformatted service failures on the task detail recovery path', () => {
    const message = taskDetailErrorMessage(
      'loadContext',
      new Error('database unavailable while loading task context')
    )

    expectBeginnerMessage(
      message,
      'Open this task again from the Tasks page to load saved notes and work history. If it still fails, ask an owner or admin to check task details access.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toMatch(new RegExp(['task', 'context'].join('\\s+'), 'i'))
  })

  test('turns cancel failures into a safe task refresh step', () => {
    const message = taskDetailErrorMessage('cancelTask', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Open this task again from the Tasks page, then choose Cancel again. The task was not canceled. If it still fails, ask an owner or admin to check task action access.'
    )
    expect(message).not.toContain('HTTP 500')
  })

  test('turns needs-help failures into a safe task refresh step', () => {
    const message = taskDetailErrorMessage('blockTask', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Open this task again from the Tasks page, then choose Needs help again. The task was not marked as needing help. If it still fails, ask an owner or admin to check task action access.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('blocked')
  })

  test('turns running-task details into a wait step', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', {
        error: 'Task is already running',
      }),
      'This task is already in progress. Wait for the current work to finish, then open this task again from the Tasks page.'
    )
  })

  test('maps nested running-task details to a wait step', () => {
    const message = taskDetailErrorMessage('retryTask', {
      error: { message: 'Task is already running' },
    })

    expectBeginnerMessage(
      message,
      'This task is already in progress. Wait for the current work to finish, then open this task again from the Tasks page.'
    )
    expect(message).not.toContain('Task is already running')
  })

  test('describes context send failures without publish wording', () => {
    const message = taskDetailErrorMessage('publishTask', new Error('HTTP 500'))

    expect(message).toContain('Check the selected saved notes, then send the task again.')
    expect(message).toContain('The task was not sent with selected notes.')
    expect(message).not.toMatch(
      new RegExp(['published', 'publish', ['selected', 'context'].join('\\s+')].join('|'), 'i')
    )
    expect(message).not.toContain('HTTP 500')
  })

  test('keeps task action fallbacks next-step first', () => {
    expect(taskDetailErrorMessage('approveTask', new Error('unexpected action issue'))).toBe(
      'Check that the task is still waiting for your decision, then choose Allow and continue again. The task did not continue.'
    )
    expect(taskDetailErrorMessage('retryTask', new Error('unknown retry issue'))).toBe(
      'Open this task again from the Tasks page, then choose Retry task again. The task was not retried.'
    )
  })

  test('turns agent and saved-note validation details into task-detail recovery steps', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('publishTask', new Error('agent required')),
      'Choose an agent, then send the task again.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('publishTask', new Error('context selection changed')),
      'Check the selected saved notes, then send the task again.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('publishTask', new Error('publish failed')),
      'Check the task details, then send the task again.'
    )
  })

  test('turns approval validation details into the Approve action', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('approveTask', new Error('approval state changed')),
      'Check that the task is still waiting for your decision, then choose Allow and continue again.'
    )
  })

  test('starts changed and missing task errors with the recovery step', () => {
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('HTTP 404')),
      'Open the Tasks page, then choose the current task again. This task was not found.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('HTTP 409')),
      'Open this task again from the Tasks page, then choose Retry task again. The task was not retried. This task changed while you were working.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('retryTask', new Error('HTTP 429')),
      'Open this task again from the Tasks page, then choose Retry task again. The task was not retried. Wait a moment before choosing Retry task again. Task actions are busy right now.'
    )
    expectBeginnerMessage(
      taskDetailErrorMessage('loadRuns', new Error('HTTP 429')),
      'Wait a moment, then open this task again from the Tasks page. Task details are busy right now.'
    )
  })
})
