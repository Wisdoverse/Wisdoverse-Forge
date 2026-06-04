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
      'Sign in again, then retry this approval queue action.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = approvalQueueErrorMessage('loadQueue', new TypeError('Failed to fetch'))

    expect(message).toContain('approval queue could not load')
    expect(message).toContain('app could not reach the service')
    expect(message).not.toContain('API')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear conflict recovery step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', new Error('409 conflict')),
      'This candidate changed while you were reviewing it. Refresh the queue, then open it again.'
    )
  })

  test('turns service failures into reusable context setup recovery', () => {
    const message = approvalQueueErrorMessage('loadQueue', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'The approval queue is temporarily unavailable. Refresh the queue, then try again. If it still fails, ask an owner or admin to check reusable context setup.'
    )
    expect(message).not.toContain('backend')
  })

  test('turns validation details into a scope next step', () => {
    expectBeginnerMessage(
      approvalQueueErrorMessage('approveCandidate', {
        error: 'Scope ID is required',
      }),
      'Choose the scope and review the source preview, then try again.'
    )
  })
})
