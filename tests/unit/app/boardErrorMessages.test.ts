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
      'Sign in again, then open the board and try this action again.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = boardActionErrorMessage('loadReadiness', new TypeError('Failed to fetch'))

    expect(message).toContain('Refresh the board to load agent status before sending work.')
    expect(message).toContain('If it still does not load, check your connection')
    const previousActionPhrase = ['assigning', 'or', 'publishing', 'work'].join(' ')
    expect(message).not.toContain(previousActionPhrase)
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a clear next step when no agent can preview context', () => {
    expect(boardActionErrorMessage('previewContext', new Error('No available agent'))).toBe(
      'No agent can prepare the saved item preview right now. Open Agents to start or connect an agent, then return to the board and refresh.'
    )
  })

  test('uses saved item wording when board send fails', () => {
    const message = boardActionErrorMessage('publishTask', new Error('HTTP 500'))

    expect(message).toContain(
      'Review the saved item preview, then send the task with selected saved items again.'
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
      'Refresh the board to load tasks. If it still fails, ask an owner or admin to check task board setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('uses saved item preview recovery wording when preview fails', () => {
    const message = boardActionErrorMessage('previewContext', new Error('HTTP 500'))

    expect(message).toContain('Choose an available agent, then open the saved item preview again.')
    expect(message).not.toMatch(new RegExp(['context', 'preview'].join('\\s+'), 'i'))
    expect(message).not.toContain('HTTP 500')
  })

  test('keeps moved-back task failures actionable without repeating the refresh step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('moveTask', new Error('HTTP 500')),
      'Refresh the board, then move the task again. The task was moved back because the board change was not saved. If it still fails, ask an owner or admin to check task board setup.'
    )
  })

  test('turns validation details into a concrete field recovery step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('createTask', {
        error: 'Task title is required',
      }),
      'Add a task result, choose the project and task queue, then create the task again.'
    )
  })
})
