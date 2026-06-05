import { describe, expect, test } from 'vitest'
import { platformKeyErrorMessage } from '@app/features/settings/platformKeyErrorMessage'

describe('platformKeyErrorMessage', () => {
  test('turns permission errors into an owner or admin step', () => {
    expect(
      platformKeyErrorMessage(
        'You do not have permission to create the platform API key. Code: 403. Details: Forbidden'
      )
    ).toBe(
      'Platform access key could not be created. Ask an owner or admin for access to manage platform access keys.'
    )
  })

  test('explains missing names as the next field to fix', () => {
    expect(
      platformKeyErrorMessage(
        'Check the required fields for platform API key, then try again. Code: 422. Details: name is required'
      )
    ).toBe(
      'Platform access key could not be created. Enter a short name that says where this key will be used, then try again.'
    )
  })

  test('explains duplicate keys with a safe next action', () => {
    expect(platformKeyErrorMessage('API 409 duplicate key')).toBe(
      'Platform access key could not be created. A key with this name or value already exists. Refresh the list, then choose a different name or revoke the old key first.'
    )
  })

  test('explains network failures in user-facing terms', () => {
    expect(platformKeyErrorMessage(new TypeError('Failed to fetch'))).toBe(
      'Platform access keys could not be loaded. The app could not reach the service. Check your connection, then try again.'
    )
  })
})
