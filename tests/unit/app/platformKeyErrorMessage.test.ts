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
      'Platform access key could not be created. Ask an owner or admin to let you create or remove platform access keys.'
    )
  })

  test('explains missing names as the next field to fix', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage(
        'Check the required fields for platform API key, then try again. Code: 422. Details: name is required'
      ),
      'Platform access key could not be created. Enter the tool or job name, then try again.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage('API 409 duplicate key'),
      'Platform access key could not be created. A platform access key with this name already exists. Refresh the list, then choose a different name or remove the old key first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = platformKeyErrorMessage(new TypeError('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Platform access keys could not be loaded. Forge could not connect while opening platform access key settings. Check your connection, then try again.'
    )
    expect(message).not.toContain('the service')
    expect(message).not.toContain('Failed to fetch')
  })

  test('turns temporary failures into a platform access key settings recovery step', () => {
    const message = platformKeyErrorMessage('HTTP 500')

    expectBeginnerMessage(
      message,
      'Platform access keys could not be loaded. Refresh Settings, then try again. If it still fails, ask an owner or admin to check platform access key settings.'
    )
    expect(message).not.toContain('access key service')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns structured rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      platformKeyErrorMessage({ statusCode: '429' }),
      'Platform access keys could not be loaded. Forge is receiving too many platform access key requests right now. Wait a minute, then try again.'
    )
  })

  test('turns unknown details into an owner or admin setup step', () => {
    const message = platformKeyErrorMessage({ message: 'unexpected platform key parser detail' })

    expectBeginnerMessage(
      message,
      'Platform access keys could not be loaded. Try again. If it still fails, ask an owner or admin to check platform access key settings.'
    )
    expect(message).not.toContain('parser')
  })
})
