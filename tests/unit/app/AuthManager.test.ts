/**
 * @vitest-environment jsdom
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

import { AuthManager } from '@app/shared/auth/AuthManager'

const LOGIN_NETWORK_FALLBACK =
  'Check your connection, then choose Sign in again. Forge could not connect.'
const REGISTER_NETWORK_FALLBACK =
  'Check your connection, then create the account again. Forge could not connect.'

function makeManager(): AuthManager {
  localStorage.clear()
  return new AuthManager('http://localhost:4003')
}

/**
 * Build a non-expired, structurally valid JWT string. `AuthManager` decodes the
 * payload without verifying the signature, so a base64url `header.payload.sig`
 * with a future `exp` is enough for `loadFromStorage` to accept it.
 */
function makeFakeJwt(): string {
  const b64url = (obj: unknown) =>
    btoa(JSON.stringify(obj)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  const header = b64url({ alg: 'HS256', typ: 'JWT' })
  const payload = b64url({
    exp: Math.floor(Date.now() / 1000) + 3600,
    orgId: 'org-1',
    role: 'owner',
  })
  return `${header}.${payload}.sig`
}

/**
 * Seed a logged-in, NON-admin user into storage then construct a manager whose
 * `this.user` is populated (so `fetchMe` won't short-circuit on a missing user).
 */
function makeAuthedManager(): AuthManager {
  localStorage.clear()
  localStorage.setItem('af:auth:access', makeFakeJwt())
  localStorage.setItem(
    'af:auth:user',
    JSON.stringify({
      id: 'u1',
      email: 'dev@example.com',
      username: 'dev',
      role: 'owner',
      isAdmin: false,
    })
  )
  return new AuthManager('http://localhost:4003')
}

describe('AuthManager beginner-safe errors', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    localStorage.clear()
  })

  function mockAuthFailure() {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        json: vi.fn().mockResolvedValue({ ok: false }),
      })
    )
  }

  it('returns an actionable login message when Forge cannot connect', async () => {
    const manager = makeManager()
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    const result = await manager.login('dev@example.com', 'password')

    expect(result.error).toBe(LOGIN_NETWORK_FALLBACK)
    expect(result.error).not.toContain('try signing in')
    expect(result.error).not.toContain('Network error')
    manager.dispose()
  })

  it('returns an actionable login fallback when the server gives no message', async () => {
    const manager = makeManager()
    mockAuthFailure()

    const result = await manager.login('dev@example.com', 'password')

    expect(result.error).toBe(
      'Check your email and password, then choose Sign in again. Forge could not finish sign-in.'
    )
    expect(result.error).not.toContain('try signing in')
    expect(result.error).not.toContain('Login failed')
    manager.dispose()
  })

  it('returns an actionable registration message when Forge cannot connect', async () => {
    const manager = makeManager()
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    const result = await manager.register('dev@example.com', 'password')

    expect(result.error).toBe(REGISTER_NETWORK_FALLBACK)
    expect(result.error).not.toContain('Network error')
    manager.dispose()
  })

  it('returns an actionable registration fallback when the server gives no message', async () => {
    const manager = makeManager()
    mockAuthFailure()

    const result = await manager.register('dev@example.com', 'password')

    expect(result.error).toBe(
      'Check the account details, then create the account again. Forge could not finish account creation.'
    )
    expect(result.error).not.toContain('account setup')
    expect(result.error).not.toContain('Registration failed')
    manager.dispose()
  })

  it('throws actionable account-recovery fallbacks when the server gives no message', async () => {
    const manager = makeManager()
    mockAuthFailure()

    await expect(manager.exchangeAuthCode('code')).rejects.toThrow(
      'Start sign-in again from this page. Forge could not finish this sign-in link.'
    )
    await expect(manager.resendVerification('dev@example.com')).rejects.toThrow(
      'Check the email address, then send the verification email again. Forge could not finish sending it.'
    )
    await expect(manager.forgotPassword('dev@example.com')).rejects.toThrow(
      'Check the email address, then request the reset email again. Forge could not finish sending it.'
    )
    await expect(manager.resetPassword('token', 'new-password')).rejects.toThrow(
      'Check the password rules, then save the new password again. Forge could not finish password reset.'
    )
    manager.dispose()
  })
})

describe('AuthManager.fetchMe platform-admin hydration (#881)', () => {
  // fetchMe is the load-bearing source of the global `isAdmin` flag (the JWT
  // does NOT carry it). It must fail closed — leaving the user un-elevated and
  // returning null — on any non-happy path, and elevate ONLY on a strict
  // `data.isAdmin === true`. The `/admin` route guard reads the resulting cached
  // user, and the backend re-enforces the real gate, so a truthy-but-non-boolean
  // value must never grant admin access.
  afterEach(() => {
    vi.restoreAllMocks()
    localStorage.clear()
  })

  function mockMe(response: unknown, ok = true) {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok,
        json: vi.fn().mockResolvedValue(response),
      })
    )
  }

  it('returns null and stays un-elevated when there is no access token', async () => {
    const manager = makeManager() // no token seeded
    const fetchSpy = vi.fn()
    vi.stubGlobal('fetch', fetchSpy)

    const result = await manager.fetchMe()

    expect(result).toBeNull()
    expect(fetchSpy).not.toHaveBeenCalled() // short-circuits before any request
    manager.dispose()
  })

  it('returns null and stays un-elevated when the response is not ok', async () => {
    const manager = makeAuthedManager()
    mockMe({ ok: true, isAdmin: true }, /* ok */ false)

    const result = await manager.fetchMe()

    expect(result).toBeNull()
    expect(manager.getUser()?.isAdmin).toBe(false)
    manager.dispose()
  })

  it('returns null and stays un-elevated when data.ok !== true', async () => {
    const manager = makeAuthedManager()
    mockMe({ ok: false, isAdmin: true })

    const result = await manager.fetchMe()

    expect(result).toBeNull()
    expect(manager.getUser()?.isAdmin).toBe(false)
    manager.dispose()
  })

  it('returns null and stays un-elevated when fetch throws', async () => {
    const manager = makeAuthedManager()
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    const result = await manager.fetchMe()

    expect(result).toBeNull()
    expect(manager.getUser()?.isAdmin).toBe(false)
    manager.dispose()
  })

  it('does NOT elevate on a truthy non-boolean isAdmin (server sends "true")', async () => {
    const manager = makeAuthedManager()
    mockMe({ ok: true, user_id: 'u1', org_id: 'org-1', role: 'owner', isAdmin: 'true' })

    const result = await manager.fetchMe()

    // The merge still succeeds, but isAdmin is normalized to a strict false.
    expect(result).not.toBeNull()
    expect(result?.isAdmin).toBe(false)
    expect(manager.getUser()?.isAdmin).toBe(false)
    manager.dispose()
  })

  it('elevates ONLY on a strict boolean isAdmin === true', async () => {
    const manager = makeAuthedManager()
    mockMe({ ok: true, user_id: 'u1', org_id: 'org-1', role: 'owner', isAdmin: true })

    const result = await manager.fetchMe()

    expect(result?.isAdmin).toBe(true)
    expect(manager.getUser()?.isAdmin).toBe(true)
    // Persists the elevated flag so the synchronous `/admin` route guard sees it.
    expect(JSON.parse(localStorage.getItem('af:auth:user') ?? '{}').isAdmin).toBe(true)
    manager.dispose()
  })
})
