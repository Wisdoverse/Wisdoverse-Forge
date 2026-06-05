import { describe, expect, test } from 'vitest'
import { providerTestErrorMessage } from '@app/features/settings/providerTestErrorMessage'

describe('providerTestErrorMessage', () => {
  test('turns invalid key details into setup guidance', () => {
    expect(providerTestErrorMessage('Invalid key', 'Anthropic Review')).toBe(
      'Anthropic Review connection check failed. Check the service access key, model, and service address, then save and check again.'
    )
  })

  test('turns permission failures into access key and model guidance', () => {
    expect(providerTestErrorMessage(new Error('HTTP 403: Forbidden'), 'OpenAI Production')).toBe(
      'OpenAI Production connection check failed. Confirm the saved service access key is active and allowed to use the selected model, then save and check again.'
    )
  })

  test('turns network failures into service address guidance', () => {
    const message = providerTestErrorMessage(new TypeError('Failed to fetch'), 'Local Lab')

    expect(message).toBe(
      'Local Lab connection check failed. Forge could not connect to this model service. Check the service address and your connection, then check again.'
    )
    expect(message).not.toContain('network access')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns temporary check failures into owner-check guidance', () => {
    const message = providerTestErrorMessage(new Error('HTTP 500'), 'OpenAI Production')

    expect(message).toBe(
      'OpenAI Production connection check failed. Model service checks are temporarily unavailable. Try again in a few minutes. If it still fails, ask an owner to check model service settings.'
    )
    expect(message).not.toContain('gateway')
  })
})
