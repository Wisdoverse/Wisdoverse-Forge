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

    expect(message).toContain('Agent status could not load')
    expect(message).toContain('Refresh the board before sending work')
    expect(message).toContain('Forge could not connect while loading the board')
    const previousActionPhrase = ['assigning', 'or', 'publishing', 'work'].join(' ')
    expect(message).not.toContain(previousActionPhrase)
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a clear next step when no agent can preview context', () => {
    expect(boardActionErrorMessage('previewContext', new Error('No available agent'))).toBe(
      'No agent is available for saved item preview. Start an agent or wait for one to finish, then try again.'
    )
  })

  test('uses saved item wording when board send fails', () => {
    const message = boardActionErrorMessage('publishTask', new Error('HTTP 500'))

    expect(message).toContain('The task was not sent with selected saved items.')
    expect(message).toContain('Review the saved item preview, then try again.')
    expect(message).not.toMatch(
      new RegExp(
        ['published', 'publish', ['context', 'preview'].join('\\s+'), ['published', 'with', 'context'].join('\\s+')].join('|'),
        'i'
      )
    )
  })

  test('turns service failures into a task board setup recovery step', () => {
    const message = boardActionErrorMessage('loadTasks', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'The task board could not load. Refresh the board, then try again. Forge could not load the board right now. Refresh the board, then try again. If it still fails, ask an owner or admin to check task board setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
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
