import { describe, expect, test } from 'vitest'

import { userRoleLabel } from '@app/entities/user'

describe('userRoleLabel', () => {
  test('turns missing access levels into a refresh step', () => {
    expect(userRoleLabel(null)).toBe('Refresh access level')
    expect(userRoleLabel(' ')).toBe('Refresh access level')
  })

  test('keeps known and unexpected access levels readable', () => {
    expect(userRoleLabel('owner')).toBe('Owner')
    expect(userRoleLabel('super-admin')).toBe('Check access level')
  })
})
