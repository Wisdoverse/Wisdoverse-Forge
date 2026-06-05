import { describe, expect, test } from 'vitest'
import { platformKeyErrorMessage } from '@app/features/settings/platformKeyErrorMessage'

describe('platformKeyErrorMessage', () => {
  test('turns permission errors into an owner or admin step', () => {
    expect(
      platformKeyErrorMessage(
        'You do not have permission to create the platform API key. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Platform access key could not be created. Ask an owner or admin to let you create or remove platform access keys.'
    )
  })

  test('explains missing names as the next field to fix', () => {
    expect(
      platformKeyErrorMessage(
        'Check the required fields for platform API key, then try again. Code: 422. Details: name is required'
      )
    ).toBe(
      'Platform access key could not be created. Enter the app, script, or workflow name, then try again.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    expect(platformKeyErrorMessage('API 409 duplicate key')).toBe(
      'Platform access key could not be created. A platform access key with this name already exists. Refresh the list, then choose a different name or remove the old key first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    const message = platformKeyErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Platform access keys could not be loaded. The app could not reach platform access key settings. Check your connection, then try again.'
    )
    expect(message).not.toContain('the service')
  })

  test('turns temporary failures into a platform access key settings recovery step', () => {
    const message = platformKeyErrorMessage('HTTP 500')

    expect(message).toBe(
      'Platform access keys could not be loaded. Platform access key settings are temporarily unavailable. Try again. If it still fails, ask an owner to check platform access key settings.'
    )
    expect(message).not.toContain('access key service')
  })
})
