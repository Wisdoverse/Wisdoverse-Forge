import { describe, expect, test } from 'vitest'
import { feedbackErrorMessage } from '@app/entities/context/model/feedbackErrorMessage'

describe('feedbackErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
  }

  test('turns auth failures into a sign-in step', () => {
    expectBeginnerMessage(
      feedbackErrorMessage(new Error('API 401: {"message":"token expired"}')),
      'Sign in again, then save this feedback.'
    )
  })

  test('turns validation details into a feedback choice step', () => {
    const message = feedbackErrorMessage(new Error('HTTP 422: {"message":"vote is required"}'))

    expectBeginnerMessage(
      message,
      'Choose one feedback option for this context item, then try again.'
    )
    expect(message).not.toContain('HTTP 422')
    expect(message).not.toContain('vote is required')
  })

  test('turns server failures into an admin recovery step', () => {
    expectBeginnerMessage(
      feedbackErrorMessage(new Error('HTTP 503: {"message":"database unavailable"}')),
      'Feedback is temporarily unavailable. Refresh the task, then try again. If it still fails, ask an admin to check feedback setup.'
    )
  })
})
