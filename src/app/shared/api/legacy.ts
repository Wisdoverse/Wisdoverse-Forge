/**
 * Legacy API Adapter
 *
 * Wraps the legacy-compatible API factory functions with the React app's
 * auth context, providing a singleton-per-session accessor pattern for React
 * components.
 *
 * URL conventions:
 *   - Agent, settings, user APIs: apiUrl = '/api/v1'
 *   - Billing API: apiUrl = '/api' (billing routes are at /api/billing/...,
 *     and the factory already appends /billing/ to the URL)
 *
 * Error handling:
 *   - AgentAPI methods return { ok: false, error } on failure — they never throw.
 *   - SettingsAPI / BillingAPI / UserAPI throw typed error classes
 *     (SettingsApiError / BillingApiError / UserApiError) on failure.
 *   Callers must handle each pattern accordingly — this adapter does NOT
 *   normalise errors.
 *
 * Lifecycle:
 *   1. Call initLegacyApis(authManager) once after successful login/refresh.
 *   2. Use getAgentApi() / getSettingsApi() / getBillingApi() / getUserApi()
 *      in React components and hooks.
 *   3. Call resetLegacyApis() on logout to clear instances.
 */

import type { AuthManager } from '@app/shared/auth/AuthManager'
import { createAgentAPI, type AgentAPI } from './legacy/AgentAPI'
import { createSettingsAPI, type SettingsAPI } from './legacy/settingsApi'
import { createBillingAPI, type BillingAPI } from './legacy/billingApi'
import { createUserAPI, type UserAPI } from './legacy/userApi'
import { createAuthFetch } from './legacy/authFetch'

// ---------------------------------------------------------------------------
// Module-level singletons (one set per login session)
// ---------------------------------------------------------------------------

let _authFetchFn: typeof fetch | null = null
let _agentApi: AgentAPI | null = null
let _settingsApi: SettingsAPI | null = null
let _billingApi: BillingAPI | null = null
let _userApi: UserAPI | null = null

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Initialise all legacy API instances bound to the given AuthManager.
 * Safe to call multiple times — subsequent calls are no-ops if already
 * initialised for the same session.  Call resetLegacyApis() first if you
 * need to re-initialise (e.g. after org-switch or re-login).
 */
export function initLegacyApis(
  authManager: AuthManager,
  onAuthExpired: () => void = () => undefined
): void {
  if (_agentApi !== null) return // already initialised

  const authFetch = createAuthFetch({ authManager, onAuthExpired })
  _authFetchFn = authFetch
  const getAuthHeaders = () => authManager.getAuthHeader()

  const v1Url = '/api/v1'
  const baseUrl = '/api'

  _agentApi = createAgentAPI(v1Url, getAuthHeaders, authFetch)
  _settingsApi = createSettingsAPI(v1Url, getAuthHeaders, authFetch)
  _billingApi = createBillingAPI(baseUrl, getAuthHeaders, authFetch)
  _userApi = createUserAPI(v1Url, getAuthHeaders, authFetch)
}

/**
 * Reset all singleton instances.  Call on logout so the next login creates
 * fresh instances bound to the new session's tokens.
 */
export function resetLegacyApis(): void {
  _authFetchFn = null
  _agentApi = null
  _settingsApi = null
  _billingApi = null
  _userApi = null
}

/** Get the auth-aware fetch function (handles 401 refresh). Throws if not initialized. */
export function getAuthFetch(): typeof fetch {
  if (!_authFetchFn) throw new Error('Legacy APIs not initialised — call initLegacyApis() first')
  return _authFetchFn
}

/**
 * Returns the AgentAPI instance.
 * AgentAPI methods return { ok: false, error } on failure — they never throw.
 *
 * @throws Error if called before initLegacyApis()
 */
export function getAgentApi(): AgentAPI {
  if (!_agentApi) throw new Error('AgentAPI not initialised — call initLegacyApis() first')
  return _agentApi
}

/**
 * Returns the SettingsAPI instance.
 * SettingsAPI methods throw SettingsApiError on network/server errors.
 *
 * @throws Error if called before initLegacyApis()
 */
export function getSettingsApi(): SettingsAPI {
  if (!_settingsApi) throw new Error('SettingsAPI not initialised — call initLegacyApis() first')
  return _settingsApi
}

/**
 * Returns the BillingAPI instance.
 * BillingAPI methods throw BillingApiError on network/server errors.
 *
 * @throws Error if called before initLegacyApis()
 */
export function getBillingApi(): BillingAPI {
  if (!_billingApi) throw new Error('BillingAPI not initialised — call initLegacyApis() first')
  return _billingApi
}

/**
 * Returns the UserAPI instance.
 * UserAPI methods throw UserApiError on network/server errors.
 *
 * @throws Error if called before initLegacyApis()
 */
export function getUserApi(): UserAPI {
  if (!_userApi) throw new Error('UserAPI not initialised — call initLegacyApis() first')
  return _userApi
}
