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
      'Sign in again, then choose the feedback option again.'
    )
  })

  test('turns permission failures into saved item access guidance', () => {
    const message = feedbackErrorMessage(new Error('HTTP 403: Forbidden'))

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to this saved item, then choose the feedback option again. You do not have permission to save feedback for this saved item.'
    )
    expect(message).not.toContain('role')
    expect(message).not.toContain('HTTP 403')
    expect(message).not.toContain('Forbidden')
  })

  test('turns role-required failures into saved item access guidance', () => {
    const message = feedbackErrorMessage('owner role required')

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to give you access to this saved item, then choose the feedback option again. You do not have permission to save feedback for this saved item.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns validation details into a feedback choice step', () => {
    const message = feedbackErrorMessage(new Error('HTTP 422: {"message":"vote is required"}'))

    expectBeginnerMessage(
      message,
      'Choose Useful, Outdated, Incorrect, Too sensitive, or Do not use again for this saved item, then choose it again.'
    )
    expect(message).not.toContain('HTTP 422')
    expect(message).not.toContain('vote is required')
    expect(message).not.toContain('try again')
  })

  test('turns network failures into connection guidance', () => {
    const message = feedbackErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then choose the feedback option again. Forge could not connect while saving it.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
  })

  test('turns server failures into an owner or admin recovery step', () => {
    const message = feedbackErrorMessage(new Error('HTTP 503: {"message":"database unavailable"}'))

    expectBeginnerMessage(
      message,
      'Open task details again, then choose the feedback option again. Forge could not save feedback right now. If it still fails, ask an owner or admin to check saved item feedback access.'
    )
    expect(message).not.toContain('HTTP 503')
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('Refresh the task')
  })

  test('keeps unformatted service failures on the feedback recovery path', () => {
    const message = feedbackErrorMessage(new Error('database unavailable while saving feedback'))

    expectBeginnerMessage(
      message,
      'Open task details again, then choose the feedback option again. Forge could not save feedback right now. If it still fails, ask an owner or admin to check saved item feedback access.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Choose Useful')
  })

  test('turns changed saved items into a task details retry step', () => {
    expectBeginnerMessage(
      feedbackErrorMessage({ status: 409 }),
      'Open task details again, check this saved item, then choose the feedback option again. This saved item changed while you were giving feedback.'
    )
  })

  test('turns missing saved items into a task details step', () => {
    const message = feedbackErrorMessage({ statusCode: 404 })

    expectBeginnerMessage(
      message,
      'Open task details again, choose this saved item again, then choose the feedback option again. This saved item could not be found.'
    )
    expect(message).not.toContain('Refresh the task')
  })

  test('turns missing saved-item context into a task details step', () => {
    const message = feedbackErrorMessage(new Error('HTTP 422: {"message":"context missing"}'))

    expectBeginnerMessage(
      message,
      'Open task details again, choose the saved item again, then choose the feedback option again.'
    )
    expect(message).not.toContain('Refresh the task')
  })

  test('turns busy feedback into a wait step', () => {
    expectBeginnerMessage(
      feedbackErrorMessage({ statusCode: 429 }),
      'Wait a moment, then choose the feedback option again. Feedback is busy.'
    )
  })

  test('keeps unknown save failures actionable', () => {
    expectBeginnerMessage(
      feedbackErrorMessage({ status: 418 }),
      'Open task details again, then choose the feedback option again. Feedback could not be saved.'
    )
  })
})
