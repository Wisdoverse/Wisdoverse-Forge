/**
 * Shared API infrastructure types used across multiple layers.
 *
 * These types are consumed by both the `entities/agent` API and the `shared`
 * layer stores. They live here so the `shared` layer does not need to import
 * from `entities` (which would violate FSD boundaries).
 *
 * Do NOT put agent business-logic types here — those belong in
 * `src/app/entities/agent/`.
 */

/**
 * Standardised error fields returned by the server's error-handler plugin.
 * All API responses may include these when ok=false.
 */
export interface ApiErrorFields {
  error?: string
  message?: string
  details?: { reason?: string; issues?: Array<{ path: string; message: string }> }
  requestId?: string
}

/**
 * Extract a human-readable error message from any API error response.
 * Priority: details.reason > message > error code > fallback.
 */
export function extractApiError(
  data: ApiErrorFields,
  fallback = 'Forge did not return a clear error. Refresh, then try again.'
): string {
  const rawError = (data as Record<string, unknown>).error
  const nestedError =
    rawError && typeof rawError === 'object' && !Array.isArray(rawError)
      ? (rawError as Record<string, unknown>)
      : null
  const nestedMessage =
    typeof nestedError?.message === 'string'
      ? nestedError.message
      : typeof nestedError?.code === 'string'
        ? nestedError.code
        : undefined
  return (
    data.details?.reason ||
    data.message ||
    (typeof rawError === 'string' ? rawError : nestedMessage) ||
    fallback
  )
}

/** Provides auth headers for API requests. */
export type AuthHeaderProvider = () => Record<string, string>

// ============================================================================
// Resource Profile Types (frontend-friendly subset)
// ============================================================================

export interface ResourceProfileOption {
  id: string
  name: string
  cpu: number
  memoryMb: number
}

// ============================================================================
// User SSH Key Types
// ============================================================================

export interface UserSshKey {
  id: string
  label: string
  fingerprint: string
  keyType: string
  publicKey: string
  createdAt: string
  updatedAt: string
}

// ============================================================================
// Git Credential Types
// ============================================================================

export type GitProvider = 'gitlab' | 'github'

export interface GitCredential {
  id: string
  provider: GitProvider
  host: string | null
  createdAt: string
  updatedAt: string
}
