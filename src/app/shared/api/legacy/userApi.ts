/**
 * UserAPI - Pure API layer for user profile operations
 *
 * All functions are pure HTTP calls with no DOM/state dependencies.
 * Mirrors the server user controller endpoints at /users/me.
 */

import type { AuthHeaderProvider } from './AgentAPI'

// ============================================================================
// Types
// ============================================================================

export interface UserProfile {
  id: string
  email?: string
  name?: string
  systemUsername?: string
  role: 'admin' | 'user' | 'viewer'
  emailVerified: boolean
  hasPassword: boolean
  provider?: string
  lastLogin?: number
  createdAt: number
  updatedAt: number
}

// ============================================================================
// Error Class
// ============================================================================

export class UserApiError extends Error {
  constructor(
    message: string,
    public readonly statusCode?: number,
    public readonly serverError?: string
  ) {
    super(message)
    this.name = 'UserApiError'
  }
}

// ============================================================================
// API Factory
// ============================================================================

export function createUserAPI(
  apiUrl: string,
  getAuthHeaders?: AuthHeaderProvider,
  fetchFn: typeof fetch = fetch
) {
  function headers(extra?: Record<string, string>): Record<string, string> {
    return {
      'Content-Type': 'application/json',
      ...(getAuthHeaders?.() ?? {}),
      ...extra,
    }
  }

  function headersNoBody(): Record<string, string> {
    return getAuthHeaders?.() ?? {}
  }

  async function parseResponse<T>(
    response: Response,
    extractData: (data: Record<string, unknown>) => T
  ): Promise<T> {
    if (!response.ok) {
      let serverError: string | undefined
      try {
        const errorData = await response.json()
        if (errorData && typeof errorData === 'object') {
          serverError = errorData.error || errorData.message
        }
      } catch {
        // Response body not valid JSON — use status text as fallback
      }
      throw new UserApiError(
        `HTTP ${response.status}: ${response.statusText}`,
        response.status,
        serverError
      )
    }
    const data = await response.json()
    if (!data.ok) {
      throw new UserApiError(
        data.error || data.message || 'Server returned error',
        response.status,
        data.error
      )
    }
    return extractData(data)
  }

  return {
    async getProfile(): Promise<UserProfile> {
      const response = await fetchFn(`${apiUrl}/users/me`, { headers: headersNoBody() })
      return parseResponse(response, (data) => data.user as UserProfile)
    },

    async updateProfile(data: { name?: string; email?: string }): Promise<UserProfile> {
      const response = await fetchFn(`${apiUrl}/users/me`, {
        method: 'PUT',
        headers: headers(),
        body: JSON.stringify(data),
      })
      return parseResponse(response, (d) => d.user as UserProfile)
    },

    async changePassword(currentPassword: string, newPassword: string): Promise<void> {
      const response = await fetchFn(`${apiUrl}/users/me/password`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify({ currentPassword, newPassword }),
      })
      await parseResponse(response, () => undefined)
    },
  }
}

export type UserAPI = ReturnType<typeof createUserAPI>
