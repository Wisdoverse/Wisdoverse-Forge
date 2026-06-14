/**
 * @vitest-environment jsdom
 */

/**
 * Unit tests for the Legacy API Adapter (src/app/shared/api/legacy.ts)
 *
 * Tests cover:
 * - Singleton init / get / reset lifecycle
 * - Correct URL prefixes per API type
 * - Auth headers injected via authManager.getAuthHeader()
 * - getters throw before init
 * - resetLegacyApis clears instances so getters throw again
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import {
  initLegacyApis,
  resetLegacyApis,
  getAgentApi,
  getSettingsApi,
  getBillingApi,
  getUserApi,
} from '@app/shared/api/legacy'
import type { AuthManager } from '@app/shared/auth/AuthManager'

// ---------------------------------------------------------------------------
// Shared mock helpers
// ---------------------------------------------------------------------------

/** Minimal AuthManager stub sufficient for the adapter. */
function makeAuthManager(token = 'test-token'): AuthManager {
  return {
    getAuthHeader: vi.fn().mockReturnValue({ Authorization: `Bearer ${token}` }),
    refreshTokens: vi.fn().mockResolvedValue(true),
    logout: vi.fn(),
    isAuthenticated: vi.fn().mockReturnValue(true),
    getUser: vi.fn().mockReturnValue(null),
    onAuthChange: vi.fn(),
    dispose: vi.fn(),
  } as unknown as AuthManager
}

/** Minimal fetch mock that returns a successful { ok: true, ... } JSON body. */
function makeFetchMock(body: Record<string, unknown> = { ok: true }) {
  return vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    statusText: 'OK',
    json: () => Promise.resolve(body),
  })
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe('Legacy API adapter — lifecycle', () => {
  beforeEach(() => {
    resetLegacyApis()
  })

  afterEach(() => {
    resetLegacyApis()
  })

  it('getters throw before initLegacyApis is called', () => {
    expect(() => getAgentApi()).toThrow(/not initialised/)
    expect(() => getSettingsApi()).toThrow(/not initialised/)
    expect(() => getBillingApi()).toThrow(/not initialised/)
    expect(() => getUserApi()).toThrow(/not initialised/)
  })

  it('getters return instances after initLegacyApis', () => {
    initLegacyApis(makeAuthManager())

    expect(getAgentApi()).toBeDefined()
    expect(getSettingsApi()).toBeDefined()
    expect(getBillingApi()).toBeDefined()
    expect(getUserApi()).toBeDefined()
  })

  it('initLegacyApis is idempotent — second call is a no-op', () => {
    const am1 = makeAuthManager('token-a')
    const am2 = makeAuthManager('token-b')

    initLegacyApis(am1)
    const api1 = getAgentApi()

    initLegacyApis(am2)
    const api2 = getAgentApi()

    // Same instance — second init was ignored
    expect(api1).toBe(api2)
  })

  it('resetLegacyApis clears instances so getters throw again', () => {
    initLegacyApis(makeAuthManager())

    // Sanity: works before reset
    expect(() => getAgentApi()).not.toThrow()

    resetLegacyApis()

    expect(() => getAgentApi()).toThrow(/not initialised/)
    expect(() => getSettingsApi()).toThrow(/not initialised/)
    expect(() => getBillingApi()).toThrow(/not initialised/)
    expect(() => getUserApi()).toThrow(/not initialised/)
  })

  it('re-initialises after reset', () => {
    initLegacyApis(makeAuthManager('first'))
    const first = getAgentApi()

    resetLegacyApis()

    initLegacyApis(makeAuthManager('second'))
    const second = getAgentApi()

    // New instance after re-init
    expect(second).not.toBe(first)
  })
})

// ---------------------------------------------------------------------------
// URL prefix routing
// ---------------------------------------------------------------------------

describe('Legacy API adapter — URL prefixes', () => {
  let fetchMock: ReturnType<typeof makeFetchMock>

  beforeEach(() => {
    resetLegacyApis()
    fetchMock = makeFetchMock()
    // Stub window.navigator.onLine used by authFetch offline logic
    Object.defineProperty(globalThis, 'navigator', {
      value: { onLine: true },
      writable: true,
      configurable: true,
    })
    // Stub window.addEventListener used by authFetch (online event)
    if (!globalThis.window) {
      Object.defineProperty(globalThis, 'window', {
        value: { addEventListener: vi.fn() },
        writable: true,
        configurable: true,
      })
    } else {
      vi.spyOn(globalThis.window, 'addEventListener').mockImplementation(() => {})
    }
  })

  afterEach(() => {
    resetLegacyApis()
    vi.restoreAllMocks()
  })

  it('AgentAPI calls /api/v1 prefix', async () => {
    const am = makeAuthManager()
    initLegacyApis(am, () => {})

    // Swap out the underlying fetch that authFetch delegates to
    vi.stubGlobal('fetch', fetchMock)

    await getAgentApi().getServerInfo()

    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining('/api/v1/'), expect.any(Object))
  })

  it('SettingsAPI calls /api/v1 prefix', async () => {
    const am = makeAuthManager()
    initLegacyApis(am, () => {})

    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: () => Promise.resolve({ ok: true, providers: [] }),
    })
    vi.stubGlobal('fetch', fetchMock)

    await getSettingsApi().getSupportedProviders()

    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining('/api/v1/'), expect.any(Object))
  })

  it('BillingAPI calls /api prefix (not /api/v1)', async () => {
    const am = makeAuthManager()
    initLegacyApis(am, () => {})

    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: () => Promise.resolve({ ok: true, plans: [] }),
    })
    vi.stubGlobal('fetch', fetchMock)

    await getBillingApi().getPlans()

    const calledUrl = fetchMock.mock.calls[0][0] as string
    expect(calledUrl).toContain('/api/billing/')
    expect(calledUrl).not.toContain('/api/v1/')
  })

  it('UserAPI calls /api/v1 prefix', async () => {
    const am = makeAuthManager()
    initLegacyApis(am, () => {})

    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      statusText: 'OK',
      json: () =>
        Promise.resolve({
          ok: true,
          user: {
            id: 'u1',
            role: 'user',
            emailVerified: true,
            hasPassword: true,
            createdAt: 0,
            updatedAt: 0,
          },
        }),
    })
    vi.stubGlobal('fetch', fetchMock)

    await getUserApi().getProfile()

    expect(fetchMock).toHaveBeenCalledWith(expect.stringContaining('/api/v1/'), expect.any(Object))
  })
})

describe('Legacy API adapter — beginner-safe fallback errors', () => {
  beforeEach(() => {
    resetLegacyApis()
    Object.defineProperty(globalThis, 'navigator', {
      value: { onLine: true },
      writable: true,
      configurable: true,
    })
    if (!globalThis.window) {
      Object.defineProperty(globalThis, 'window', {
        value: { addEventListener: vi.fn() },
        writable: true,
        configurable: true,
      })
    } else {
      vi.spyOn(globalThis.window, 'addEventListener').mockImplementation(() => {})
    }
  })

  afterEach(() => {
    resetLegacyApis()
    vi.restoreAllMocks()
  })

  it('uses an actionable request fallback when the server gives no clear message', async () => {
    initLegacyApis(makeAuthManager(), () => {})
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 503,
        statusText: 'Service Unavailable',
        json: () => Promise.resolve({}),
      })
    )

    const result = await getAgentApi().createUserLlmConfig({} as never)

    expect(result.error).toBe('Forge could not finish this request. Wait a moment, then try again.')
    expect(result.error).not.toContain('Server error')
  })

  it('uses an actionable network fallback when the request cannot connect', async () => {
    initLegacyApis(makeAuthManager(), () => {})
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')))

    const result = await getAgentApi().testUserLlmConfig('cfg_1')

    expect(result.error).toBe('Check your connection, then try again. Forge could not connect.')
    expect(result.error).not.toContain('Network error')
  })
})

// ---------------------------------------------------------------------------
// Auth header injection
// ---------------------------------------------------------------------------

describe('Legacy API adapter — auth header injection', () => {
  beforeEach(() => {
    resetLegacyApis()
    Object.defineProperty(globalThis, 'navigator', {
      value: { onLine: true },
      writable: true,
      configurable: true,
    })
    if (!globalThis.window) {
      Object.defineProperty(globalThis, 'window', {
        value: { addEventListener: vi.fn() },
        writable: true,
        configurable: true,
      })
    } else {
      vi.spyOn(globalThis.window, 'addEventListener').mockImplementation(() => {})
    }
  })

  afterEach(() => {
    resetLegacyApis()
    vi.restoreAllMocks()
  })

  it('AgentAPI requests include Authorization header from authManager', async () => {
    const am = makeAuthManager('my-secret-token')
    initLegacyApis(am, () => {})

    const fetchMock = makeFetchMock()
    vi.stubGlobal('fetch', fetchMock)

    await getAgentApi().getServerInfo()

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    const headers = init.headers as Record<string, string>
    expect(headers['Authorization']).toBe('Bearer my-secret-token')
  })
})
