import { describe, expect, test } from 'vitest'
import { settingsMembershipErrorMessage } from '@app/pages/settings/ui/settingsMembershipErrorMessage'

describe('settingsMembershipErrorMessage', () => {
  test('turns permission failures into an owner or admin next step', () => {
    expect(settingsMembershipErrorMessage('HTTP 403', { resource: 'teams', action: 'load' })).toBe(
      'Teams could not be loaded. Ask an owner or admin for access to manage teams.'
    )
  })

  test('turns duplicate create failures into a name change next step', () => {
    expect(
      settingsMembershipErrorMessage('Code: 409 already exists', {
        resource: 'projects',
        action: 'create',
      })
    ).toBe('Projects could not be created. Use a different name, then try again.')
  })

  test('turns server failures into a backend recovery step', () => {
    expect(
      settingsMembershipErrorMessage('Server error 500', {
        resource: 'projects',
        action: 'load',
      })
    ).toBe(
      'Projects could not be loaded. The workspace settings service is temporarily unavailable. Ask an owner to check the backend, then try again.'
    )
  })

  test('turns network failures into a connection step', () => {
    expect(
      settingsMembershipErrorMessage(new TypeError('Failed to fetch'), {
        resource: 'teams',
        action: 'create',
      })
    ).toBe(
      'Teams could not be created. The browser could not reach the server. Check your connection, then try again.'
    )
  })
})
