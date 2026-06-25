import { getAuthFetch } from './legacy'

/**
 * The shared auth-aware fetch used by every feature API client.
 *
 * Routes through the AuthManager-bound singleton (`createAuthFetch`): it injects
 * the current access token, includes the httpOnly refresh cookie
 * (`credentials: 'include'`), and on a 401 refreshes the session ONCE and
 * retries the original request (with offline queueing). That means a token that
 * expires mid-session — e.g. between scheduled refreshes while a tab is
 * backgrounded — recovers transparently instead of surfacing a raw `API 401`
 * and bouncing the operator to re-login (F068/F075).
 *
 * Falls back to a plain token-stamped fetch ONLY when the legacy APIs are not
 * yet initialised (very early boot or unit tests), so callers never throw on the
 * "not initialised" guard. Once `initLegacyApis()` has run after login/refresh,
 * every call gets the full refresh-and-retry behaviour.
 */
export function authFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  try {
    return getAuthFetch()(input, init)
  } catch {
    const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
    const headers: Record<string, string> = {
      ...(init?.headers as Record<string, string> | undefined),
    }
    if (token) headers.Authorization = `Bearer ${token}`
    return fetch(input, { credentials: 'include', ...init, headers })
  }
}
