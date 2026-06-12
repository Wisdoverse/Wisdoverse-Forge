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
      'AI service could not be saved. Paste the service access key from the selected AI service, then save again.'
    )
  })

  test('turns missing model errors into a model step', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage('HTTP 422: model is required'),
      'AI service could not be saved. Keep the suggested model or choose a supported model, then save again.'
    )
  })

  test('turns missing service address errors into an address step', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage('HTTP 422: base_url is required'),
      'AI service could not be saved. Add the service address for this AI service, then save again.'
    )
  })

  test('uses server error details for missing service addresses', () => {
    const message = providerSettingsErrorMessage({
      serverError: 'base url is required',
      statusCode: 422,
    })

    expectBeginnerMessage(
      message,
      'AI service could not be saved. Add the service address for this AI service, then save again.'
    )
    expect(message).not.toContain('base url is required')
  })

  test('turns permission errors into an owner or admin step', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage(
        'You do not have permission to save the provider. Code: 403. Details: Forbidden'
      ),
      'AI service could not be saved. Ask an owner or admin to let you manage AI services.'
    )
  })

  test('explains duplicate providers with a safe next action', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage('API 409 duplicate provider'),
      'AI service could not be saved. An AI service with this name or setup already exists. Refresh the list, then choose a different name or remove the old service first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = providerSettingsErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'AI service settings could not be loaded. Forge could not connect while opening AI service settings. Check your connection, then try again.'
    )
    expect(message).not.toContain('the service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns temporary failures into a model service settings recovery step', () => {
    const message = providerSettingsErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'AI service settings could not be loaded. Refresh Settings, then try again. If it still fails, ask an owner or admin to check AI service settings.'
    )
    expect(message).not.toContain('settings page')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      providerSettingsErrorMessage({ statusCode: '429' }),
      'AI service settings could not be loaded. Forge is receiving too many AI service requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown failures into an owner or admin setup step', () => {
    const message = providerSettingsErrorMessage({ message: 'unexpected provider parser error' })

    expectBeginnerMessage(
      message,
      'AI service settings could not be loaded. Try again. If it still fails, ask an owner or admin to check AI service settings.'
    )
    expect(message).not.toContain('parser')
  })
})
