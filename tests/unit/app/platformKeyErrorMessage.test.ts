import { describe, expect, test } from 'vitest'
import { platformKeyErrorMessage } from '@app/features/settings/platformKeyErrorMessage'

describe('platformKeyErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns permission errors into an owner or admin step', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage(
        'You do not have permission to create the platform API key. Code: 403. Details: Forbidden'
      ),
      'Ask an owner or admin to let you create or remove outside tool access keys.'
    )
  })

  test('explains missing names as the next field to fix', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage(
        'Check the required fields for platform API key, then try again. Code: 422. Details: name is required'
      ),
      'Enter the tool or job name, then try again.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    const message = platformKeyErrorMessage('API 409 duplicate key')

    expectBeginnerMessage(
      message,
      'Refresh the list, then choose a different name or remove the old key first. An outside tool access key with this name already exists.'
    )
    expect(message).not.toContain('Outside tool access key could not be created')
  })

  test('explains network failures in user-facing terms', () => {
    const message = platformKeyErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then refresh Settings to load outside tool access keys. Forge could not connect while opening outside tool access settings.'
    )
    expect(message).not.toContain('the service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('starts create network failures with the recovery step', () => {
    const message = platformKeyErrorMessage('creating platform key failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then create this outside tool access key again. The creation did not finish.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('opening outside tool access settings')
  })

  test('starts remove network failures with the recovery step', () => {
    const message = platformKeyErrorMessage('removing platform key failed: Network error')

    expectBeginnerMessage(
      message,
      'Check your connection, then remove this outside tool access key again. The removal did not finish.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('opening outside tool access settings')
  })

  test('turns temporary failures into an outside tool access settings recovery step', () => {
    const message = platformKeyErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Refresh Settings to load outside tool access keys. If it still fails, ask an owner or admin to check outside tool access settings.'
    )
    expect(message).not.toContain('access key service')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage({ statusCode: '429' }),
      'Wait a minute, then try again. Forge is receiving too many outside tool access requests right now.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = platformKeyErrorMessage({ message: 'unexpected platform key parser detail' })

    expectBeginnerMessage(
      message,
      'Refresh Settings to load outside tool access keys. If it still fails, ask an owner or admin to check outside tool access settings.'
    )
    expect(message).not.toContain('parser')
  })
})
