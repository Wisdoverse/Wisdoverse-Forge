import { describe, expect, test } from 'vitest'
import { runtimeErrorMessage } from '@app/features/settings/runtimeErrorMessages'

describe('runtimeErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expect(runtimeErrorMessage('loadAgentSignals', new Error('401 Unauthorized'))).toBe(
      'Sign in again, then retry this runtime setup action. Code: 401.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = runtimeErrorMessage('loadCliSignIn', new TypeError('Failed to fetch'))

    expect(message).toContain('Local tool sign-in status could not load')
    expect(message).toContain('browser could not reach the server')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear permission step for local sign-in startup', () => {
    expect(runtimeErrorMessage('startCliSignIn', { error: '403 Forbidden' })).toBe(
      'You do not have permission to manage runtime setup. Ask an owner or admin to update your role. Code: 403.'
    )
  })

  test('keeps short validation details after the operator instruction', () => {
    expect(
      runtimeErrorMessage('startCliSignIn', {
        error: 'Provider is not configured',
      })
    ).toBe(
      'Local tool sign-in did not start. Check the provider setup, then try Connect again. Detail: Provider is not configured'
    )
  })
})
