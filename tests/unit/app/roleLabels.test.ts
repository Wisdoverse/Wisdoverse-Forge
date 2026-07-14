import { describe, expect, test } from 'vitest'

import { userRoleLabel } from '@app/entities/user'

describe('userRoleLabel', () => {
  test('turns missing access levels into an Account settings reload step', () => {
    expect(userRoleLabel(null)).toBe('Open Account settings again to load access level')
    expect(userRoleLabel(' ')).toBe('Open Account settings again to load access level')
  })

  test('keeps known and unexpected access levels readable', () => {
    expect(userRoleLabel('owner')).toBe('Owner')
    expect(userRoleLabel('operator')).toBe('Member')
    expect(userRoleLabel('super-admin')).toBe('Check access level')
  })
})
