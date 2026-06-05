import { describe, expect, test } from 'vitest'
import { providerSettingsErrorMessage } from '@app/features/settings/providerSettingsErrorMessage'

describe('providerSettingsErrorMessage', () => {
  test('turns validation errors into provider setup guidance', () => {
    expect(
      providerSettingsErrorMessage(
        'Check the required fields for provider, then try again. Code: 422. Details: API key is required'
      )
    ).toBe(
      'Model service could not be saved. Choose the AI service, confirm the model, add the service access key, and add the service address if needed. Then save again.'
    )
  })

  test('turns permission errors into an owner or admin step', () => {
    expect(
      providerSettingsErrorMessage(
        'You do not have permission to save the provider. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Model service could not be saved. Ask an owner or admin to let you manage model services.'
    )
  })

  test('explains duplicate providers with a safe next action', () => {
    expect(providerSettingsErrorMessage('API 409 duplicate provider')).toBe(
      'Model service could not be saved. A model service with this name or setup already exists. Refresh the list, then choose a different name or remove the old service first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = providerSettingsErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Model service settings could not be loaded. The app could not reach model service settings. Check your connection, then try again.'
    )
    expect(message).not.toContain('the service')
  })

  test('turns temporary failures into a model service settings recovery step', () => {
    const message = providerSettingsErrorMessage('HTTP 500')

    expect(message).toBe(
      'Model service settings could not be loaded. Model service settings are temporarily unavailable. Try again. If it still fails, ask an owner to check model service settings.'
    )
    expect(message).not.toContain('settings page')
  })
})
