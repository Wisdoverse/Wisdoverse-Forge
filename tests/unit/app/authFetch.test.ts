import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

// F068/F075: the shared auth-aware fetch must route every feature client through
// the AuthManager-bound singleton (which refreshes + retries on 401) and fall
// back safely before the legacy APIs are initialised.
const legacyMock = vi.hoisted(() => ({ getAuthFetch: vi.fn() }))
vi.mock('@app/shared/api/legacy', () => legacyMock)

import { authFetch } from '@app/shared/api/authFetch'

describe('authFetch (F068/F075)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
  })
  afterEach(() => {
    vi.restoreAllMocks()
  })

  test('routes through the AuthManager-bound singleton when initialised', async () => {
    const singleton = vi.fn().mockResolvedValue(new Response('ok'))
    legacyMock.getAuthFetch.mockReturnValue(singleton)
    const init: RequestInit = { method: 'POST' }

    await authFetch('/api/v1/skills', init)

    // The singleton (createAuthFetch) owns token injection + 401 refresh/retry,
    // so the call must be delegated to it verbatim, not re-implemented here.
    expect(legacyMock.getAuthFetch).toHaveBeenCalledTimes(1)
    expect(singleton).toHaveBeenCalledWith('/api/v1/skills', init)
  })

  test('falls back to a credentialed token-stamped fetch before init', async () => {
    legacyMock.getAuthFetch.mockImplementation(() => {
      throw new Error('Legacy APIs not initialised')
    })
    localStorage.setItem('af:auth:access', 'tok-123')
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('ok'))

    await authFetch('/api/v1/skills', { method: 'GET' })

    expect(fetchSpy).toHaveBeenCalledTimes(1)
    const [calledUrl, calledInit] = fetchSpy.mock.calls[0] as [string, RequestInit]
    expect(calledUrl).toBe('/api/v1/skills')
    expect(calledInit).toMatchObject({ credentials: 'include', method: 'GET' })
    expect(new Headers(calledInit.headers).get('Authorization')).toBe('Bearer tok-123')
  })

  test.each<{ kind: string; headers: HeadersInit }>([
    { kind: 'record', headers: { authorization: 'Bearer expired', 'X-Request-ID': 'request-1' } },
    {
      kind: 'Headers',
      headers: new Headers({ authorization: 'Bearer expired', 'X-Request-ID': 'request-1' }),
    },
    {
      kind: 'tuples',
      headers: [
        ['authorization', 'Bearer expired'],
        ['X-Request-ID', 'request-1'],
      ],
    },
  ])('fallback preserves $kind headers and replaces an expired token', async ({ headers }) => {
    legacyMock.getAuthFetch.mockImplementation(() => {
      throw new Error('not initialised')
    })
    localStorage.setItem('af:auth:access', 'current-token')
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('ok'))
    const signal = new AbortController().signal

    await authFetch('/api/v1/skills', { method: 'POST', headers, body: '{}', signal })

    const calledInit = fetchSpy.mock.calls[0][1] as RequestInit
    const sentHeaders = new Headers(calledInit.headers)
    expect(sentHeaders.get('X-Request-ID')).toBe('request-1')
    expect(sentHeaders.get('Authorization')).toBe('Bearer current-token')
    expect(calledInit).toMatchObject({ credentials: 'include', method: 'POST', body: '{}', signal })
    expect(new Headers(headers).get('Authorization')).toBe('Bearer expired')
  })

  test('fallback omits Authorization when no token is stored', async () => {
    legacyMock.getAuthFetch.mockImplementation(() => {
      throw new Error('not initialised')
    })
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('ok'))

    await authFetch('/api/v1/skills')

    const calledInit = fetchSpy.mock.calls[0][1] as RequestInit
    expect(new Headers(calledInit.headers).get('Authorization')).toBeNull()
  })
})
