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

    expect(message).toContain('Agent readiness could not load')
    expect(message).toContain('The browser could not reach the server')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear next step when no agent can preview context', () => {
    expect(boardActionErrorMessage('previewContext', new Error('No available agent'))).toBe(
      'No agent is available for context preview. Start an agent or wait for one to finish, then try again.'
    )
  })

  test('turns service failures into a task board setup recovery step', () => {
    const message = boardActionErrorMessage('loadTasks', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'The board is temporarily unavailable. Refresh the board, then try again. If it still fails, ask an owner or admin to check task board setup.'
    )
    expect(message).not.toContain('backend')
  })

  test('turns validation details into a concrete field recovery step', () => {
    expectBeginnerMessage(
      boardActionErrorMessage('createTask', {
        error: 'Task title is required',
      }),
      'Add a task title, choose the project and work lane, then create the task again.'
    )
  })
})
