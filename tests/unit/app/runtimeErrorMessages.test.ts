import { describe, expect, test } from 'vitest'
import {
  runtimeErrorMessage,
  runtimeSettingsErrorMessage,
} from '@app/features/settings/runtimeErrorMessages'

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

describe('runtimeSettingsErrorMessage', () => {
  test('turns unavailable runtime choices into a clear save step', () => {
    expect(
      runtimeSettingsErrorMessage(
        'Check the required fields for runtime setting, then try again. Code: 422. Details: default CLI tool is not available'
      )
    ).toBe(
      'Runtime settings could not be saved. Choose an available work location and local tool, then save again.'
    )
  })

  test('turns permission failures into an owner or admin step', () => {
    expect(
      runtimeSettingsErrorMessage(
        'You do not have permission to update runtime settings. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Runtime settings could not be saved. Ask an owner or admin for access to manage runtime setup.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    expect(runtimeSettingsErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Runtime settings could not be loaded. The browser could not reach the server. Check your connection, then refresh Settings.'
    )
  })
})
