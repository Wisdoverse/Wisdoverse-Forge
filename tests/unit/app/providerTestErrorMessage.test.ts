import { describe, expect, test } from 'vitest'
import { providerTestErrorMessage } from '@app/features/settings/providerTestErrorMessage'

describe('providerTestErrorMessage', () => {
  test('turns invalid key details into setup guidance', () => {
    expect(providerTestErrorMessage('Invalid key', 'Anthropic Review')).toBe(
      'Anthropic Review connection test failed. Check the secret key, model, and service address, then save and check again.'
    )
  })

  test('turns permission failures into secret key and model guidance', () => {
    expect(providerTestErrorMessage(new Error('HTTP 403: Forbidden'), 'OpenAI Production')).toBe(
      'OpenAI Production connection test failed. Check that the saved secret key is active and allowed to use the selected model, then save and check again.'
    )
  })

  test('turns network failures into service address guidance', () => {
    expect(providerTestErrorMessage(new TypeError('Failed to fetch'), 'Local Lab')).toBe(
      'Local Lab connection test failed. The platform could not reach the model service. Check network access and the service address, then check again.'
    )
  })
})
