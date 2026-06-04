import { describe, expect, test } from 'vitest'
import { boardActionErrorMessage } from '@app/features/board/boardErrorMessages'

describe('boardActionErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expect(boardActionErrorMessage('loadTasks', new Error('401 Unauthorized'))).toBe(
      'Sign in again, then retry this board action. Code: 401.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = boardActionErrorMessage('loadReadiness', new TypeError('Failed to fetch'))

    expect(message).toContain('Agent readiness could not load')
    expect(message).toContain('The browser could not reach the server')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear next step when no agent can preview context', () => {
    expect(boardActionErrorMessage('previewContext', new Error('No available agent'))).toBe(
      'No agent is available for context preview. Start an agent or wait for one to finish, then try again.'
    )
  })

  test('keeps short validation details after the safe operator instruction', () => {
    expect(
      boardActionErrorMessage('createTask', {
        error: 'Task title is required',
      })
    ).toBe(
      'The task was not created. Check the project, work lane, and title, then try again. Detail: Task title is required'
    )
  })
})
