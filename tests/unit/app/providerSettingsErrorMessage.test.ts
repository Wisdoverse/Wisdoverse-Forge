import { describe, expect, test } from 'vitest'
import { providerSettingsErrorMessage } from '@app/features/settings/providerSettingsErrorMessage'

describe('providerSettingsErrorMessage', () => {
  test('turns validation errors into provider setup guidance', () => {
    expect(
      providerSettingsErrorMessage(
        'Check the required fields for provider, then try again. Code: 422. Details: API key is required'
      )
    ).toBe(
      'Provider could not be saved. Check the provider, model, API key, and Base URL, then save again.'
    )
  })

  test('turns permission errors into an owner or admin step', () => {
    expect(
      providerSettingsErrorMessage(
        'You do not have permission to save the provider. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Provider could not be saved. Ask an owner or admin for access to manage model providers.'
    )
  })

  test('explains duplicate providers with a safe next action', () => {
    expect(providerSettingsErrorMessage('API 409 duplicate provider')).toBe(
      'Provider could not be saved. A provider with this name or configuration already exists. Refresh the list, then choose a different name or remove the old provider first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    expect(providerSettingsErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Provider settings could not be loaded. The app could not reach the service. Check your connection, then try again.'
    )
  })
})
