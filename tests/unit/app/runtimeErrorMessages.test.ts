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
      'Sign in again, then open Where agents work and try again. Your sign-in expired.'
    )
  })

  test('explains network failures without exposing only a transport error', () => {
    const message = runtimeErrorMessage('loadCliSignIn', new TypeError('Failed to fetch'))

    expect(message).toContain('Work tool sign-in could not be checked')
    expect(message).toContain('Forge could not connect while checking Where agents work')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('gives a concrete Agents step when agent online status cannot be checked', () => {
    const message = runtimeErrorMessage('loadAgentSignals', new TypeError('Failed to fetch'))

    expect(message).toContain('Open Agents and make sure one agent shows Ready')
    expect(message).toContain('Agent connection status could not load')
    expect(message).not.toContain('wake an agent')
    expect(message).not.toContain('Failed to fetch')
  })

  test('gives team space access guidance for local sign-in startup permissions', () => {
    const message = runtimeErrorMessage('startCliSignIn', { error: '403 Forbidden' })

    expectBeginnerMessage(
      message,
      'Ask an owner or admin to update your team space access before changing Where agents work. You do not have permission to change Where agents work.'
    )
    expect(message).not.toContain('role')
  })

  test('turns AI service details into a connect step', () => {
    const message = runtimeErrorMessage('startCliSignIn', {
      error: 'Provider is not configured',
    })

    expectBeginnerMessage(
      message,
      'Choose and save an AI service first, then reconnect the work tool sign-in.'
    )
    expect(message).not.toContain('provider')
  })

  test('uses connected AI service language for unclear local sign-in validation', () => {
    const message = runtimeErrorMessage('startCliSignIn', {
      error: 'setup is incomplete',
    })

    expectBeginnerMessage(
      message,
      'Check the connected AI service and selected work tool, then reconnect the work tool sign-in.'
    )
    expect(message).not.toContain('provider')
  })

  test('uses connected AI service language when local sign-in startup cannot reach service', () => {
    const message = runtimeErrorMessage('startCliSignIn', new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check the connected AI service, then reconnect the account. Work tool sign-in did not start. Check your connection, then refresh Settings. Forge could not connect while checking Where agents work.'
    )
    expect(message).not.toContain('provider')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('app could not reach')
  })

  test('turns setup service failures into a Where agents work recovery step', () => {
    const message = runtimeErrorMessage('loadAgentSignals', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh this page, then try again. Forge could not check Where agents work right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('worker')
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns setup rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('loadAgentSignals', { code: '429' }),
      'Wait a moment, then try again. Forge is receiving too many setup requests right now.'
    )
  })

  test('turns changed setup status into a current-status check step', () => {
    expectBeginnerMessage(
      runtimeErrorMessage('loadAgentSignals', { statusCode: 409 }),
      'Refresh this page, check the current status, then try again. The choices in Where agents work changed while you were working.'
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
      'Choose where project files open and a work tool, then save again. Where agents work could not be saved.'
    )
  })

  test('turns permission failures into an owner or admin step', () => {
    expect(
      runtimeSettingsErrorMessage(
        'You do not have permission to update agent work settings. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Ask an owner or admin for access to change Where agents work, then save again. Where agents work could not be saved.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = runtimeSettingsErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then refresh Settings to load Where agents work.'
    )
    expect(message).not.toContain('app could not reach')
  })

  test('turns temporary run-setting failures into a settings recovery step', () => {
    const message = runtimeSettingsErrorMessage(new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh Settings to load Where agents work. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(message).not.toContain('backend')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns work settings rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      runtimeSettingsErrorMessage({ statusCode: '429' }),
      'Wait a minute, then refresh Settings. Too many setup requests are happening right now.'
    )
  })

  test('turns unknown work settings failures into owner or admin guidance', () => {
    const message = runtimeSettingsErrorMessage({ reason: 'unexpected runtime parser detail' })

    expectBeginnerMessage(
      message,
      'Refresh Settings to load Where agents work. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(message).not.toContain('parser')
  })

  test('turns unknown save failures into a specific Where agents work recovery step', () => {
    const message = runtimeSettingsErrorMessage({
      reason: 'update runtime settings ended with an unexpected detail',
    })

    expectBeginnerMessage(
      message,
      'Check where project files open and the work tool choice, then save Where agents work again. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(message).not.toContain('unexpected')
  })

  test('turns changed work choices into a current-choices check step', () => {
    expectBeginnerMessage(
      runtimeSettingsErrorMessage({
        status: 409,
        reason: 'update runtime settings conflict',
      }),
      'Refresh Settings, check the current choices, then save again. The choices in Where agents work changed while you were working.'
    )
  })
})
