import { describe, expect, test } from 'vitest'
import { boardActionErrorMessage } from '@app/features/board/boardErrorMessages'

function expectBeginnerMessage(actual: string, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Detail:')
}

describe('boardActionErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('loadTasks', new Error('401 Unauthorized')),
      'Sign in again, then choose Check tasks again.'
    )
  })

  test('turns permission failures into board access guidance', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('moveTask', {
        status: '403',
        serverError: 'missing board policy',
      }),
      'Ask an owner or admin to give you access to the Tasks page, then move the task again. You do not have permission to change this board.'
    )
  })

  test('turns role-required failures into board access guidance', () => {
    const message = boardActionErrorMessage('moveTask', 'owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to the Tasks page, then move the task again. You do not have permission to change this board.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = boardActionErrorMessage('loadReadiness', new TypeError('Failed to fetch'))

    expect(message).toContain('Choose Check agent status before sending work.')
    expect(message).toContain('If it still does not load, check your connection')
    expect(message).toContain('choose Check agent status')
    expect(message).not.toContain('Refresh agent status')
    expect(message).not.toContain('refresh the page')
    const previousActionPhrase = ['assigning', 'or', 'publishing', 'work'].join(' ')
    expect(message).not.toContain(previousActionPhrase)
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('keeps network recovery tied to the board action', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('moveTask', new TypeError('Connection refused')),
      'Choose Check tasks again, then move the task again. The task was moved back because the board change was not saved. If it still does not update, check your connection, then move the task again.'
    )
  })

  test('turns board conflicts into a current task check step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('moveTask', new Error('HTTP 409')),
      'Choose Check tasks again so you see the latest tasks, then move the task again. The task board changed while you were working.'
    )
  })

  test('gives a clear next step when no agent can preview context', () => {
    expect(boardActionErrorMessage('previewContext', new Error('No available agent'))).toBe(
      'No agent can check saved items right now. Open Agents to start or connect an agent, then open the Tasks page and check saved items again.'
    )
  })

  test('turns rate limits into an action-specific retry step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('publishTask', new Error('429 too many requests')),
      'The board is busy with too many requests. Wait a moment, then send the task with selected saved notes again.'
    )
  })

  test('uses saved notes wording when board send fails', () => {
    const message = boardActionErrorMessage('publishTask', new Error('HTTP 500'))

    expect(message).toContain(
      'Check the saved notes, then send the task with selected saved notes again.'
    )
    expect(message).toContain('The task was not sent.')
    expect(message).not.toMatch(
      new RegExp(
        [
          'published',
          'publish',
          ['context', 'preview'].join('\\s+'),
          ['published', 'with', 'context'].join('\\s+'),
        ].join('|'),
        'i'
      )
    )
  })

  test('turns service failures into a task board setup recovery step', () => {
    const message = boardActionErrorMessage('loadTasks', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Choose Check tasks again to load tasks. If it still fails, ask an owner or admin to check task board access.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('keeps raw service details on the board action recovery path', () => {
    const message = boardActionErrorMessage(
      'createTask',
      new Error('database unavailable while creating task')
    )

    expectBeginnerMessage(
      message,
      'Add the task result, choose a project and a place for new tasks, then create the task again. The task was not created. If it still fails, ask an owner or admin to check task board actions.'
    )
    expect(message).not.toContain('task queue')
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Add a task result')
  })

  test('uses saved items recovery wording when preview fails', () => {
    const message = boardActionErrorMessage('previewContext', new Error('HTTP 500'))

    expect(message).toContain('Choose an agent, then check saved items again.')
    expect(message).not.toContain('available agent')
    expect(message).not.toMatch(new RegExp(['context', 'preview'].join('\\s+'), 'i'))
    expect(message).not.toContain('HTTP 500')
  })

  test('keeps moved-back task failures actionable without repeating the refresh step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('moveTask', new Error('HTTP 500')),
      'Choose Check tasks again, then move the task again. The task was moved back because the board change was not saved. If it still fails, ask an owner or admin to check task board actions.'
    )
  })

  test('turns validation details into a concrete field recovery step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('createTask', {
        error: 'Task title is required',
      }),
      'Add the task result, choose a project and a place for new tasks, then create the task again.'
    )
  })

  test('explains missing place for new tasks with canonical wording', () => {
    const message = boardActionErrorMessage('createTask', {
      error: 'Task group is required',
    })

    expectBeginnerMessage(
      message,
      'Choose a place for new tasks in this project, then create the task again.'
    )
    expect(message).not.toContain('task queue')
    expect(message).not.toContain('task group')
  })

  test('maps nested place-for-new-tasks validation details', () => {
    const message = boardActionErrorMessage('createTask', {
      error: { message: 'Task group is required' },
    })

    expectBeginnerMessage(
      message,
      'Choose a place for new tasks in this project, then create the task again.'
    )
    expect(message).not.toContain('task queue')
    expect(message).not.toContain('Task group is required')
  })

  test('turns missing agent validation into a saved items recovery step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('previewContext', {
        error: 'Agent is required',
      }),
      'Choose an agent, then open saved items from this task again.'
    )
  })
})
