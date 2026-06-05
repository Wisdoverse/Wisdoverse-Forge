import { describe, expect, test } from 'vitest'
import {
  runtimeErrorMessage,
  runtimeSettingsErrorMessage,
} from '@app/features/settings/runtimeErrorMessages'

function expectBeginnerMessage(actual: string, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Detail:')
  expect(actual).not.toContain('Details:')
}

describe('runtimeErrorMessage', () => {
  test('turns auth failures into a sign-in instruction', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('loadAgentSignals', new Error('401 Unauthorized')),
      'Your sign-in expired. Sign in again, then open Agent setup and try this action again.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = runtimeErrorMessage('loadCliSignIn', new TypeError('Failed to fetch'))

    expect(message).toContain('Work tool sign-in status could not load')
    expect(message).toContain('Forge could not connect while checking agent setup')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a clear permission step for local sign-in startup', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('startCliSignIn', { error: '403 Forbidden' }),
      'You do not have permission to manage agent setup. Ask an owner or admin to update your role.'
    )
  })

  test('turns model service setup details into a connect step', () => {
    const message = runtimeErrorMessage('startCliSignIn', {
      error: 'Provider is not configured',
    })

    expectBeginnerMessage(message, 'Choose and save a model service first, then try Connect again.')
    expect(message).not.toContain('provider')
  })

  test('uses model service setup language for unclear local sign-in validation', () => {
    const message = runtimeErrorMessage('startCliSignIn', {
      error: 'setup is incomplete',
    })

    expectBeginnerMessage(
      message,
      'Check the model service setup and selected local tool, then try Connect again.'
    )
    expect(message).not.toContain('provider')
  })

  test('uses model service setup language when local sign-in startup cannot reach service', () => {
    const message = runtimeErrorMessage('startCliSignIn', new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Work tool sign-in did not start. Check the model service setup, then try Connect again. Forge could not connect while checking agent setup. Check your connection, then refresh Settings.'
    )
    expect(message).not.toContain('provider')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('turns setup service failures into an agent setup recovery step', () => {
    const message = runtimeErrorMessage('loadAgentSignals', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Forge could not check agent setup right now. Refresh this setup check, then try again. If it still fails, ask an owner or admin to check agent setup.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('worker')
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns setup rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('loadAgentSignals', { code: '429' }),
      'Forge is receiving too many agent setup requests right now. Wait a moment, then try again.'
    )
  })
})

describe('runtimeSettingsErrorMessage', () => {
  test('turns unavailable work choices into a clear save step', () => {
    expect(
      runtimeSettingsErrorMessage(
        'Check the required fields for runtime setting, then try again. Code: 422. Details: default CLI tool is not available'
      )
    ).toBe(
      'Agent work settings could not be saved. Choose an available work location and local tool, then save again.'
    )
  })

  test('turns permission failures into an owner or admin step', () => {
    expect(
      runtimeSettingsErrorMessage(
        'You do not have permission to update agent work settings. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Agent work settings could not be saved. Ask an owner or admin for access to manage agent setup.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = runtimeSettingsErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Agent work settings could not be loaded. Forge could not connect while opening agent work settings. Check your connection, then refresh Settings.'
    )
    expect(message).not.toContain('app could not reach')
  })

  test('turns temporary agent work settings failures into a settings recovery step', () => {
    const message = runtimeSettingsErrorMessage(new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Agent work settings could not be loaded. Refresh Settings, then try again. If it still fails, ask an owner or admin to check agent work settings.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns work settings rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      runtimeSettingsErrorMessage({ statusCode: '429' }),
      'Agent work settings could not be loaded. Forge is receiving too many agent work settings requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown work settings failures into owner or admin guidance', () => {
    const message = runtimeSettingsErrorMessage({ reason: 'unexpected runtime parser detail' })

    expectBeginnerMessage(
      message,
      'Agent work settings could not be loaded. Try again. If it still fails, ask an owner or admin to check agent work settings.'
    )
    expect(message).not.toContain('parser')
  })
})
