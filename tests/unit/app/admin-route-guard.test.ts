/**
 * @vitest-environment jsdom
 */

import { afterEach, describe, expect, test } from 'vitest'
import { canAccessAdmin, storedIsAdmin } from '@app/routes/admin'

describe('canAccessAdmin', () => {
  // Must mirror the backend platform-admin gate
  // (`AdminService::require_platform_admin`, #881), which keys off the GLOBAL
  // `users.is_admin` flag — NOT the self-assignable per-org role.
  test('only a platform admin (isAdmin === true) is allowed', () => {
    expect(canAccessAdmin(true)).toBe(true)
  })

  test('non-admins are rejected — including a self-registered org owner', () => {
    // `false`/`undefined` cover "not a platform admin" and "not hydrated yet"
    // (the guard fails closed before `/me` resolves).
    for (const isAdmin of [false, undefined]) {
      expect(canAccessAdmin(isAdmin)).toBe(false)
    }
  })
})

describe('storedIsAdmin', () => {
  // This is the load-bearing fail-closed read used by the `/admin` route guard:
  // it parses the cached user (`af:auth:user`) and grants access ONLY on a
  // strict `isAdmin === true`. A tampered/malformed cache must never elevate;
  // the backend re-enforces the real gate regardless (defense in depth).
  afterEach(() => {
    localStorage.clear()
  })

  function setStoredUser(value: string): void {
    localStorage.setItem('af:auth:user', value)
  }

  test('returns false when no cached user exists', () => {
    expect(storedIsAdmin()).toBe(false)
  })

  test('returns false on malformed JSON', () => {
    setStoredUser('{ not valid json')
    expect(storedIsAdmin()).toBe(false)
  })

  test('returns false when isAdmin is absent (undefined)', () => {
    setStoredUser(JSON.stringify({ id: 'u1', email: 'dev@example.com' }))
    expect(storedIsAdmin()).toBe(false)
  })

  test('returns false for a truthy non-boolean isAdmin (tampered string "true")', () => {
    setStoredUser(JSON.stringify({ isAdmin: 'true' }))
    expect(storedIsAdmin()).toBe(false)
  })

  test('returns false for a truthy numeric isAdmin (tampered 1)', () => {
    setStoredUser(JSON.stringify({ isAdmin: 1 }))
    expect(storedIsAdmin()).toBe(false)
  })

  test('returns false for explicit isAdmin: false', () => {
    setStoredUser(JSON.stringify({ isAdmin: false }))
    expect(storedIsAdmin()).toBe(false)
  })

  test('returns true ONLY for the strict boolean isAdmin === true', () => {
    setStoredUser(JSON.stringify({ isAdmin: true }))
    expect(storedIsAdmin()).toBe(true)
  })
})
