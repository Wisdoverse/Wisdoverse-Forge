import { describe, expect, test } from 'vitest'
import { approvalQueueErrorMessage } from '@app/features/context/approvalQueueErrorMessages'

describe('approvalQueueErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expect(approvalQueueErrorMessage('loadQueue', new Error('401 Unauthorized'))).toBe(
      'Sign in again, then retry this approval queue action. Code: 401.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = approvalQueueErrorMessage('loadQueue', new TypeError('Failed to fetch'))

    expect(message).toContain('approval queue could not load')
    expect(message).toContain('browser could not reach the server')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear conflict recovery step', () => {
    expect(approvalQueueErrorMessage('approveCandidate', new Error('409 conflict'))).toBe(
      'This candidate changed while you were reviewing it. Refresh the queue, then open it again. Code: 409.'
    )
  })

  test('keeps short validation details after the operator instruction', () => {
    expect(
      approvalQueueErrorMessage('approveCandidate', {
        error: 'Scope ID is required',
      })
    ).toBe(
      'The candidate was not approved. Review the scope and source preview, then try again. Detail: Scope ID is required'
    )
  })
})
