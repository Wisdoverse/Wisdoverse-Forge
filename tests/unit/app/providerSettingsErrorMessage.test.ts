import { describe, expect, test } from 'vitest'
import { providerSettingsErrorMessage } from '@app/features/settings/providerSettingsErrorMessage'

describe('providerSettingsErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns validation errors into provider setup guidance', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage(
        'Check the required fields for provider, then try again. Code: 422. Details: API key is required'
      ),
      'Model service could not be saved. Choose the AI service, confirm the model, add the service access key, and add the service address if needed. Then save again.'
    )
  })

  test('turns permission errors into an owner or admin step', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage(
        'You do not have permission to save the provider. Code: 403. Details: Forbidden'
      ),
      'Model service could not be saved. Ask an owner or admin to let you manage model services.'
    )
  })

  test('explains duplicate providers with a safe next action', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage('API 409 duplicate provider'),
      'Model service could not be saved. A model service with this name or setup already exists. Refresh the list, then choose a different name or remove the old service first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = providerSettingsErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Model service settings could not be loaded. Forge could not connect while opening model service settings. Check your connection, then try again.'
    )
    expect(message).not.toContain('the service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns temporary failures into a model service settings recovery step', () => {
    const message = providerSettingsErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Model service settings could not be loaded. Refresh Settings, then try again. If it still fails, ask an owner or admin to check model service settings.'
    )
    expect(message).not.toContain('settings page')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage({ statusCode: '429' }),
      'Model service settings could not be loaded. Forge is receiving too many model service requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown failures into an owner or admin setup step', () => {
    const message = providerSettingsErrorMessage({ message: 'unexpected provider parser error' })

    expectBeginnerMessage(
      message,
      'Model service settings could not be loaded. Try again. If it still fails, ask an owner or admin to check model service settings.'
    )
    expect(message).not.toContain('parser')
  })
})
