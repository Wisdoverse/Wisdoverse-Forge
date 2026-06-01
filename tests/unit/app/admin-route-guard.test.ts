import { describe, expect, test } from 'vitest'
import { canAccessAdmin } from '@app/routes/admin'

describe('canAccessAdmin', () => {
  // Must mirror the backend `AdminService::require_admin` (owner | admin).
  test('admins and owners are allowed', () => {
    expect(canAccessAdmin('admin')).toBe(true)
    expect(canAccessAdmin('owner')).toBe(true)
  })

  test('owner is allowed — regression for the admin-only guard that redirected owners away', () => {
    expect(canAccessAdmin('owner')).toBe(true)
  })

  test('every other role and a missing role are rejected', () => {
    for (const role of ['member', 'viewer', 'billing', '', undefined]) {
      expect(canAccessAdmin(role as string | undefined)).toBe(false)
    }
  })
})
