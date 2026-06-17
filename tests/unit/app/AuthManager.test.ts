/**
 * @vitest-environment jsdom
 */

import { afterEach, describe, expect, it, vi } from 'vitest'

import { AuthManager } from '@app/shared/auth/AuthManager'

const NETWORK_FALLBACK = 'Check your connection, then try again. Forge could not connect.'

function makeManager(): AuthManager {
  localStorage.clear()
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

    expect(result.error).toBe(NETWORK_FALLBACK)
    expect(result.error).not.toContain('Network error')
    manager.dispose()
  })

  it('returns an actionable login fallback when the server gives no message', async () => {
    const manager = makeManager()
    mockAuthFailure()

    const result = await manager.login('dev@example.com', 'password')

    expect(result.error).toBe(
      'Check your email and password, then try signing in again. Forge could not finish sign-in.'
    )
    expect(result.error).not.toContain('Login failed')
    manager.dispose()
  })

  it('returns an actionable registration message when Forge cannot connect', async () => {
    const manager = makeManager()
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    const result = await manager.register('dev@example.com', 'password')

    expect(result.error).toBe(NETWORK_FALLBACK)
    expect(result.error).not.toContain('Network error')
    manager.dispose()
  })

  it('returns an actionable registration fallback when the server gives no message', async () => {
    const manager = makeManager()
    mockAuthFailure()

    const result = await manager.register('dev@example.com', 'password')

    expect(result.error).toBe(
      'Check the account details, then create the account again. Forge could not finish account setup.'
    )
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
