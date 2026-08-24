import { authFetch } from './authFetch'

export interface AuthProviderInfo {
  name: string
  displayName: string
}

/**
 * Configured sign-in providers for the instance (empty = email/password only).
 * Best-effort: the Operations page shows the sign-in mode without failing
 * when the endpoint is unavailable.
 */
export async function listAuthProviders(): Promise<AuthProviderInfo[]> {
  try {
    const res = await authFetch('/api/v1/auth/providers')
    if (!res.ok) return []
    const data = (await res.json()) as { ok?: boolean; providers?: AuthProviderInfo[] }
    return Array.isArray(data.providers) ? data.providers : []
  } catch {
    return []
  }
}
