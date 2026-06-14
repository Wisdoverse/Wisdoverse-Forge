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

  it('returns an actionable login message when Forge cannot connect', async () => {
    const manager = makeManager()
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    const result = await manager.login('dev@example.com', 'password')

    expect(result.error).toBe(NETWORK_FALLBACK)
    expect(result.error).not.toContain('Network error')
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
})
