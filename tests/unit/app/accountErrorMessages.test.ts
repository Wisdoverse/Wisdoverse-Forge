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
    expect(message).not.toContain('Code: 401.')
    expect(message).not.toContain('HTTP 401')
    expect(message).not.toContain('token expired')
  })

  test('turns password validation details into a recovery step', () => {
    const error = Object.assign(new Error('HTTP 422: Unprocessable Entity'), {
      statusCode: 422,
      serverError: 'Current password is incorrect.',
    })

    const message = accountErrorMessage('changePassword', error)

    expect(message).toBe(
      'The current password did not match this account. Re-enter the current password, then try again.'
    )
    expect(message).not.toContain('Code: 422.')
    expect(message).not.toContain('Details:')
    expect(message).not.toContain('HTTP 422')
  })

  test('maps organization permission failures to an owner or admin action', () => {
    const message = accountErrorMessage('renameOrganization', new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission to rename this organization')
    expect(message).toContain('Ask an owner or admin')
    expect(message).not.toContain('Code: 403.')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('turns organization validation details into a recovery step', () => {
    const message = accountErrorMessage(
      'renameOrganization',
      Object.assign(new Error('HTTP 422'), {
        statusCode: 422,
        serverError: 'organization name already exists',
      })
    )

    expect(message).toBe(
      'That organization name is already in use. Choose a different display name, then try again.'
    )
    expect(message).not.toContain('Details:')
  })
})
