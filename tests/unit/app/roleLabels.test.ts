import { describe, expect, test } from 'vitest'

import { userRoleLabel } from '@app/entities/user'

describe('userRoleLabel', () => {
  test('turns missing access levels into an Account settings check step', () => {
    expect(userRoleLabel(null)).toBe('Check access in Account settings')
    expect(userRoleLabel(' ')).toBe('Check access in Account settings')
  })

  test('keeps known and unexpected access levels readable', () => {
    expect(userRoleLabel('owner')).toBe('Owner')
    expect(userRoleLabel('operator')).toBe('Member')
    expect(userRoleLabel('super-admin')).toBe('Check access level')
  })
})
