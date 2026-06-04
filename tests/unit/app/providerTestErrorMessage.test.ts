import { describe, expect, test } from 'vitest'
import { providerTestErrorMessage } from '@app/features/settings/providerTestErrorMessage'

describe('providerTestErrorMessage', () => {
  test('turns invalid key details into setup guidance', () => {
    expect(providerTestErrorMessage('Invalid key', 'Anthropic Review')).toBe(
      'Anthropic Review connection test failed. Check the API key, model, and Base URL, then save and test again.'
    )
  })

  test('turns permission failures into API key and model guidance', () => {
    expect(providerTestErrorMessage(new Error('HTTP 403: Forbidden'), 'OpenAI Production')).toBe(
      'OpenAI Production connection test failed. Check that the saved API key is active and allowed to use the selected model, then save and test again.'
    )
  })

  test('turns network failures into Base URL guidance', () => {
    expect(providerTestErrorMessage(new TypeError('Failed to fetch'), 'Local Lab')).toBe(
      'Local Lab connection test failed. The platform could not reach the provider. Check network access and the Base URL, then test again.'
    )
  })
})
