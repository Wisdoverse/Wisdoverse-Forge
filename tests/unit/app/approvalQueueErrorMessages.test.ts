import { describe, expect, test } from 'vitest'
import { approvalQueueErrorMessage } from '@app/features/context/approvalQueueErrorMessages'

describe('approvalQueueErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Detail:')
  }

  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('loadQueue', new Error('401 Unauthorized')),
      'Sign in again, then retry this review action.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = approvalQueueErrorMessage('loadQueue', new TypeError('Failed to fetch'))

    expect(message).toContain('Check your connection, then refresh saved notes review')
    expect(message).toContain('Forge could not connect while loading saved notes and instructions')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('explains saved review network failures with the retry first', () => {
    const message = approvalQueueErrorMessage(
      'approveCandidate',
      new TypeError('NetworkError when attempting to fetch resource')
    )

    expectBeginnerMessage(
      message,
      'Check your connection, then try this review action again. Forge could not connect while saving this review decision.'
    )
    expect(message).not.toContain('NetworkError')
  })

  test('gives a clear conflict recovery step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', new Error('409 conflict')),
      'Refresh the list, then open this item again. It changed while you were reviewing it.'
    )
  })

  test('gives a refresh step when the saved item is missing', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('rejectCandidate', new Error('404 not found')),
      'Refresh the list so you see the latest saved items. This item was not found.'
    )
  })

  test('turns service failures into saved notes setup recovery', () => {
    const message = approvalQueueErrorMessage('loadQueue', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh the list so you see the latest saved items. Saved notes review could not load. If it still fails, ask an owner or admin to check saved notes setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('keeps permission guidance in saved note wording', () => {
    const message = approvalQueueErrorMessage('rejectCandidate', new Error('403 Forbidden'))

    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('approve saved notes and instructions')
    expect(message).toContain('then retry this review action')
    expect(message).not.toContain('403 Forbidden')
    expect(message).not.toContain(['saved', 'memories'].join(' '))
  })

  test('turns validation details into a scope next step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', {
        error: 'Scope ID is required',
      }),
      'Choose who can reuse it and review the original task preview, then try again.'
    )
  })

  test('turns load validation details into a refresh step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('loadQueue', {
        detail: 'Scope ID is required',
      }),
      'Refresh the list, then check who can reuse the selected items. Saved notes review could not load.'
    )
  })

  test('turns rate limits into a wait step first', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('loadQueue', new Error('429 too many requests')),
      'Wait a moment, then try again. Saved notes review is busy.'
    )
  })
})
