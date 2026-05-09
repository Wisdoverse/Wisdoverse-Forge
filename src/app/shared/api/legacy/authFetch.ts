/**
 * authFetch - Fetch wrapper that handles 401 responses with automatic token refresh
 *
 * - Intercepts 401 responses and triggers token refresh
 * - Concurrent 401s share a single refresh attempt (via AuthManager mutex)
 * - On successful refresh, retries the original request with new token
 * - On failed refresh, calls onAuthExpired callback
 * - Queues requests when offline, auto-flushes when back online
 */

import type { AuthManager } from '@app/shared/auth/AuthManager'

export interface AuthFetchOptions {
  authManager: AuthManager
  onAuthExpired: () => void
}

export type AuthFetchFn = typeof fetch

interface QueuedRequest {
  input: RequestInfo | URL
  init: RequestInit | undefined
  resolve: (value: Response) => void
  reject: (reason: unknown) => void
}

const MAX_QUEUE_SIZE = 50

export function createAuthFetch({ authManager, onAuthExpired }: AuthFetchOptions): AuthFetchFn {
  const pendingQueue: QueuedRequest[] = []

  function flushQueue() {
    const items = pendingQueue.splice(0)
    for (const item of items) {
      // Re-invoke authFetch so 401 handling still applies
      authFetch(item.input, item.init).then(item.resolve, item.reject)
    }
  }

  window.addEventListener('online', flushQueue)

  async function authFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
    // Inject current auth header and include cookies (refresh token lives in httpOnly cookie)
    const authHeaders = authManager.getAuthHeader()
    const mergedInit: RequestInit = {
      credentials: 'include',
      ...init,
      headers: {
        ...authHeaders,
        ...(init?.headers as Record<string, string>),
      },
    }

    let response: Response
    try {
      response = await fetch(input, mergedInit)
    } catch (err) {
      // Network error while offline → queue the request
      if (!navigator.onLine) {
        return new Promise<Response>((resolve, reject) => {
          if (pendingQueue.length >= MAX_QUEUE_SIZE) {
            const oldest = pendingQueue.shift()
            oldest?.reject(new Error('Offline queue overflow: request discarded'))
          }
          pendingQueue.push({ input, init, resolve, reject })
        })
      }
      throw err
    }

    if (response.status !== 401) return response

    // Got 401 - attempt refresh
    const refreshed = await authManager.refreshTokens()
    if (!refreshed) {
      onAuthExpired()
      return response
    }

    // Retry with new token
    const newAuthHeaders = authManager.getAuthHeader()
    const retryInit: RequestInit = {
      credentials: 'include',
      ...init,
      headers: {
        ...newAuthHeaders,
        ...(init?.headers as Record<string, string>),
      },
    }
    return fetch(input, retryInit)
  }

  return authFetch
}
