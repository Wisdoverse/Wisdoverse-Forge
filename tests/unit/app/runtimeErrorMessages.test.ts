import { describe, expect, test } from 'vitest'
import {
  runtimeErrorMessage,
  runtimeSettingsErrorMessage,
} from '@app/features/settings/runtimeErrorMessages'

function expectBeginnerMessage(actual: string, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Detail:')
}

describe('runtimeErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('loadAgentSignals', new Error('401 Unauthorized')),
      'Sign in again, then open Runtime setup and try this action again.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = runtimeErrorMessage('loadCliSignIn', new TypeError('Failed to fetch'))

    expect(message).toContain('Local tool sign-in status could not load')
    expect(message).toContain('app could not reach the service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives a clear permission step for local sign-in startup', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('startCliSignIn', { error: '403 Forbidden' }),
      'You do not have permission to manage runtime setup. Ask an owner or admin to update your role.'
    )
  })

  test('turns provider setup details into a connect step', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('startCliSignIn', {
        error: 'Provider is not configured',
      }),
      'Choose and save a provider first, then try Connect again.'
    )
  })

  test('turns runtime service failures into a runner recovery step', () => {
    const message = runtimeErrorMessage('loadAgentSignals', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Runtime setup is temporarily unavailable. Refresh this setup check, then try again. If it still fails, ask an owner or admin to check the runner.'
    )
    expect(message).not.toContain('backend')
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
      'Runtime settings could not be loaded. The app could not reach the service. Check your connection, then refresh Settings.'
    )
  })

  test('turns runtime settings service failures into a settings recovery step', () => {
    const message = runtimeSettingsErrorMessage(new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Runtime settings could not be loaded. The runtime settings service is temporarily unavailable. Refresh Settings, then try again. If it still fails, ask an owner to check runtime settings.'
    )
    expect(message).not.toContain('backend')
  })
})
