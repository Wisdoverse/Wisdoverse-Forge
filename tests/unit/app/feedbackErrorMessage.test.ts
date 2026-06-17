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

  test('turns permission failures into saved item access guidance', () => {
    const message = feedbackErrorMessage(new Error('HTTP 403: Forbidden'))

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to this saved item, then save feedback again. You do not have permission to save feedback for this saved item.'
    )
    expect(message).not.toContain('role')
    expect(message).not.toContain('HTTP 403')
    expect(message).not.toContain('Forbidden')
  })

  test('turns validation details into a feedback choice step', () => {
    const message = feedbackErrorMessage(new Error('HTTP 422: {"message":"vote is required"}'))

    expectBeginnerMessage(
      message,
      'Choose one feedback option for this saved item, then try again.'
    )
    expect(message).not.toContain('HTTP 422')
    expect(message).not.toContain('vote is required')
  })

  test('turns network failures into connection guidance', () => {
    const message = feedbackErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then save this feedback again. Forge could not connect while saving it.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
  })

  test('turns server failures into an owner or admin recovery step', () => {
    const message = feedbackErrorMessage(new Error('HTTP 503: {"message":"database unavailable"}'))

    expectBeginnerMessage(
      message,
      'Refresh the task, then save feedback again. Forge could not save feedback right now. If it still fails, ask an owner or admin to check feedback setup.'
    )
    expect(message).not.toContain('HTTP 503')
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns changed saved items into a refresh and retry step', () => {
    expectBeginnerMessage(
      feedbackErrorMessage({ status: 409 }),
      'Refresh the task, review this saved item, then save feedback again. This saved item changed while you were giving feedback.'
    )
  })

  test('turns busy feedback into a wait step', () => {
    expectBeginnerMessage(
      feedbackErrorMessage({ statusCode: 429 }),
      'Wait a moment, then save this feedback again. Feedback is busy.'
    )
  })

  test('keeps unknown save failures actionable', () => {
    expectBeginnerMessage(
      feedbackErrorMessage({ status: 418 }),
      'Refresh the task, then save feedback again. Feedback could not be saved.'
    )
  })
})
