import { describe, expect, test } from 'vitest'
import { providerTestErrorMessage } from '@app/features/settings/providerTestErrorMessage'

describe('providerTestErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('HTTP')
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toMatch(/\bfails?\b|\bfailed\b/i)
  }

  test('turns invalid key details into setup guidance', () => {
    expectBeginnerMessage(
      providerTestErrorMessage('Invalid key', 'Anthropic Review'),
      'Anthropic Review connection check needs attention. Check the service access key, model, and service address, then save and check again.'
    )
  })

  test('turns permission failures into access key and model guidance', () => {
    expectBeginnerMessage(
      providerTestErrorMessage(new Error('HTTP 403: Forbidden'), 'OpenAI Production'),
      'OpenAI Production connection check needs attention. Confirm the saved service access key is active and allowed to use the selected model, then save and check again.'
    )
  })

  test('turns network failures into service address guidance', () => {
    const message = providerTestErrorMessage(new TypeError('Failed to fetch'), 'Local Lab')

    expectBeginnerMessage(
      message,
      'Local Lab connection check needs attention. Forge could not connect to this model service. Check the service address and your connection, then check again.'
    )
    expect(message).not.toContain('network access')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns server check failures into owner or admin guidance', () => {
    const message = providerTestErrorMessage(new Error('HTTP 500'), 'OpenAI Production')

    expectBeginnerMessage(
      message,
      'OpenAI Production connection check needs attention. Forge could not check this model service right now. Try again in a few minutes. If it still needs attention, ask an owner or admin to check model service settings.'
    )
    expect(message).not.toContain('gateway')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and check step', () => {
    expectBeginnerMessage(
      providerTestErrorMessage({ status: 429 }, 'OpenAI Production'),
      'OpenAI Production connection check needs attention. This model service is receiving too many checks right now. Wait a minute, then check again.'
    )
  })

  test('turns unknown failures into a review and owner or admin step', () => {
    const message = providerTestErrorMessage({ reason: 'unexpected provider gateway detail' })

    expectBeginnerMessage(
      message,
      'Model service connection check needs attention. Review the model service settings, then check again. If it still needs attention, ask an owner or admin to check model service settings.'
    )
    expect(message).not.toContain('gateway')
  })
})
