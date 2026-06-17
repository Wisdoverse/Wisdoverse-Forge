import { describe, expect, test } from 'vitest'
import { accountErrorMessage } from '@app/features/settings/accountErrorMessages'

describe('accountErrorMessage', () => {
  function expectBeginnerMessage(actual: string, expected: string): void {
    expect(actual).toBe(expected)
    expect(actual).not.toContain('Code:')
    expect(actual).not.toContain('Details:')
    expect(actual).not.toContain('HTTP')
  }

  test('turns password network failures into connection guidance', () => {
    const message = accountErrorMessage('changePassword', new Error('Failed to fetch'))

    expectBeginnerMessage(
      message,
      'Check your connection, then change your password again. Forge could not connect while opening password settings.'
    )
    expect(message).toContain('Check your connection')
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('service')
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
      'Re-enter the current password, then try again. The current password did not match this account.'
    )
    expect(message).not.toContain('Code: 422.')
    expect(message).not.toContain('Details:')
    expect(message).not.toContain('HTTP 422')
  })

  test('maps team space permission failures to an owner or admin action', () => {
    const message = accountErrorMessage('renameOrganization', new Error('API 403: Forbidden'))

    expect(message).toContain('You do not have permission to rename this team space')
    expect(message).toContain('Ask an owner or admin to update your team space access')
    expect(message).not.toContain('role')
    expect(message).not.toContain('organization')
    expect(message).not.toContain('Code: 403.')
    expect(message).not.toContain('API 403')
    expect(message).not.toContain('Forbidden')
  })

  test('turns account settings failures into a retry and owner step', () => {
    const message = accountErrorMessage('renameOrganization', new Error('HTTP 500'))

    expectBeginnerMessage(
      message,
      'Refresh Settings, then rename the team space again. If it still fails, ask an owner or admin to check account settings.'
    )
    expect(message).not.toContain('Team space name could not be saved')
    expect(message).not.toContain('Organization')
    expect(message).not.toContain('organization')
    expect(message).not.toContain('backend')
    expect(message).not.toContain('service')
    expect(message).not.toContain('temporarily unavailable')
  })

  test('turns team space validation details into a recovery step', () => {
    const message = accountErrorMessage(
      'renameOrganization',
      Object.assign(new Error('HTTP 422'), {
        statusCode: 422,
        serverError: 'organization name already exists',
      })
    )

    expect(message).toBe(
      'Choose a different display name, then try again. That team space name is already in use.'
    )
    expect(message).not.toContain('organization')
    expect(message).not.toContain('Details:')
  })

  test('turns account rate limits into a wait and retry step', () => {
    expectBeginnerMessage(
      accountErrorMessage('changePassword', { statusCode: 429 }),
      'Wait a moment, then change your password again. Forge is receiving too many account settings requests right now.'
    )
  })

  test('turns unsupported account status into an owner or admin setup step', () => {
    const message = accountErrorMessage('renameOrganization', { status: 418 })

    expectBeginnerMessage(
      message,
      'Refresh Settings, then rename the team space again. If it still fails, ask an owner or admin to check account settings.'
    )
    expect(message).not.toContain('Account settings could not')
  })
})
