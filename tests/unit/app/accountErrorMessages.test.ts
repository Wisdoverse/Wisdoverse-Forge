import { describe, expect, test } from 'vitest'
import { accountErrorMessage } from '@app/features/settings/accountErrorMessages'

describe('accountErrorMessage', () => {
  test('turns password network failures into connection guidance', () => {
    const message = accountErrorMessage('changePassword', new Error('Failed to fetch'))

    expect(message).toContain('browser could not reach the server')
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
  })

  test('maps password auth failures without raw HTTP text', () => {
    const message = accountErrorMessage(
      'changePassword',
      new Error('HTTP 401: {"message":"token expired"}')
    )

    expect(message).toContain('Sign in again')
    expect(message).toContain('Code: 401.')
    expect(message).not.toContain('HTTP 401')
    expect(message).not.toContain('token expired')
  })

  test('keeps useful password validation details', () => {
    const error = Object.assign(new Error('HTTP 422: Unprocessable Entity'), {
      statusCode: 422,
      serverError: 'Current password is incorrect.',
    })

    const message = accountErrorMessage('changePassword', error)

    expect(message).toContain('Check the current password')
    expect(message).toContain('Code: 422.')
    expect(message).toContain('Details: Current password is incorrect.')
    expect(message).not.toContain('HTTP 422')
  })

  test('maps organization permission failures to an owner or admin action', () => {
    const message = accountErrorMessage('renameOrganization', new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission to rename this organization')
    expect(message).toContain('Ask an owner or admin')
    expect(message).toContain('Code: 403.')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })
})
