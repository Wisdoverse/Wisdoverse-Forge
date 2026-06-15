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
      'Check the service access key, model, and service address for Anthropic Review, then save and check again.'
    )
  })

  test('turns permission failures into access key and model guidance', () => {
    expectBeginnerMessage(
      providerTestErrorMessage(new Error('HTTP 403: Forbidden'), 'OpenAI Production'),
      'Confirm the saved service access key can use the selected model for OpenAI Production, then save and check again.'
    )
  })

  test('turns network failures into service address guidance', () => {
    const message = providerTestErrorMessage(new TypeError('Failed to fetch'), 'Local Lab')

    expectBeginnerMessage(
      message,
      'Check the service address and your connection, then check Local Lab again. Forge could not connect to this AI service.'
    )
    expect(message).not.toContain('network access')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns server check failures into owner or admin guidance', () => {
    const message = providerTestErrorMessage(new Error('HTTP 500'), 'OpenAI Production')

    expectBeginnerMessage(
      message,
      'Try checking OpenAI Production again in a few minutes. If it still cannot be checked, ask an owner or admin to check AI service settings. Forge could not check this AI service right now.'
    )
    expect(message).not.toContain('needs attention')
    expect(message).not.toContain('gateway')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and check step', () => {
    expectBeginnerMessage(
      providerTestErrorMessage({ status: 429 }, 'OpenAI Production'),
      'Wait a minute, then check OpenAI Production again. This AI service is receiving too many checks right now.'
    )
  })

  test('turns unknown failures into a review and owner or admin step', () => {
    const message = providerTestErrorMessage({ reason: 'unexpected provider gateway detail' })

    expectBeginnerMessage(
      message,
      'Review the AI service settings, then check this AI service again. If it still cannot be checked, ask an owner or admin to check AI service settings.'
    )
    expect(message).not.toContain('needs attention')
    expect(message).not.toContain('gateway')
  })
})
