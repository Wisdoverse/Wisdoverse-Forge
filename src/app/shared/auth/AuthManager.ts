/**
 * AuthManager - Token storage, refresh, and authentication state management
 *
 * Features:
 * - Dynamic JWT expiry calculation from server-returned expiresIn
 * - Multi-tab sync via storage events
 * - Remember Me support (longer refresh token)
 */

export interface AuthUser {
  id: string
  email: string
  username: string
  orgId?: string
  role?: string
}

export interface LoginResult {
  ok: boolean
  error?: string
  errorCode?: string
  user?: AuthUser
  tokens?: {
    accessToken: string
    expiresIn: number
  }
}

type AuthChangeCallback = (authenticated: boolean) => void

const STORAGE_KEYS = {
  access: 'af:auth:access',
  user: 'af:auth:user',
  rememberMe: 'af:auth:rememberMe',
  /** @deprecated Refresh token now lives in an httpOnly cookie — key only used to clear legacy values. */
  legacyRefresh: 'af:auth:refresh',
} as const

const AUTH_NETWORK_ERROR = 'Check your connection, then try again. Forge could not connect.'
const AUTH_LOGIN_FALLBACK =
  'Check your email and password, then try signing in again. Forge could not finish sign-in.'
const AUTH_REGISTER_FALLBACK =
  'Check the account details, then create the account again. Forge could not finish account setup.'
const AUTH_SSO_EXCHANGE_FALLBACK =
  'Start sign-in again from this page. Forge could not finish this sign-in link.'
const AUTH_RESEND_VERIFICATION_FALLBACK =
  'Check the email address, then send the verification email again. Forge could not finish sending it.'
const AUTH_FORGOT_PASSWORD_FALLBACK =
  'Check the email address, then request the reset email again. Forge could not finish sending it.'
const AUTH_RESET_PASSWORD_FALLBACK =
  'Check the password rules, then save the new password again. Forge could not finish password reset.'

export class AuthManager {
  private accessToken: string | null = null
  private user: AuthUser | null = null
  private refreshTimer: ReturnType<typeof setTimeout> | null = null
  private callbacks: AuthChangeCallback[] = []
  private apiUrl: string
  private refreshPromise: Promise<boolean> | null = null
  private onStorageChange: (e: StorageEvent) => void

  constructor(apiUrl: string) {
    this.apiUrl = apiUrl
    this.loadFromStorage()
    if (this.accessToken) {
      this.scheduleRefresh()
    }

    // Multi-tab sync: listen for storage changes from other tabs
    this.onStorageChange = (e: StorageEvent) => {
      if (e.key === STORAGE_KEYS.access) {
        if (e.newValue === null) {
          // Another tab logged out
          this.accessToken = null
          this.user = null
          this.clearRefreshTimer()
          this.notifyCallbacks()
        } else {
          // Another tab refreshed the token
          this.accessToken = e.newValue
          const userJson = localStorage.getItem(STORAGE_KEYS.user)
          this.user = userJson ? JSON.parse(userJson) : this.user
          this.scheduleRefresh()
        }
      } else if (e.key === STORAGE_KEYS.user && e.newValue !== null) {
        try {
          this.user = JSON.parse(e.newValue)
        } catch {
          /* ignore parse errors */
        }
      }
    }
    window.addEventListener('storage', this.onStorageChange)
  }

  dispose(): void {
    window.removeEventListener('storage', this.onStorageChange)
    this.clearRefreshTimer()
  }

  /** Decode JWT payload without verification (claims are already server-signed). */
  private parseJwtPayload(token: string): Record<string, unknown> | null {
    try {
      const parts = token.split('.')
      if (parts.length !== 3) return null
      const payload = atob(parts[1].replace(/-/g, '+').replace(/_/g, '/'))
      return JSON.parse(payload)
    } catch {
      return null
    }
  }

  /** Enrich user object with orgId/role from JWT when missing (backward compat). */
  private enrichUserFromToken(): void {
    if (!this.user || !this.accessToken) return
    const claims = this.parseJwtPayload(this.accessToken)
    if (!claims) return
    if (!this.user.orgId && typeof claims.orgId === 'string') {
      this.user.orgId = claims.orgId
    }
    if (!this.user.role && typeof claims.role === 'string') {
      this.user.role = claims.role
    }
  }

  private loadFromStorage(): void {
    try {
      // Evict legacy refresh token from localStorage — it now lives in an httpOnly cookie.
      localStorage.removeItem(STORAGE_KEYS.legacyRefresh)
      this.accessToken = localStorage.getItem(STORAGE_KEYS.access)
      const userJson = localStorage.getItem(STORAGE_KEYS.user)
      this.user = userJson ? JSON.parse(userJson) : null
      this.enrichUserFromToken()
      if (this.user) this.saveToStorage()
    } catch {
      this.clearStorage()
    }
  }

  private saveToStorage(): void {
    if (this.accessToken) {
      localStorage.setItem(STORAGE_KEYS.access, this.accessToken)
    } else {
      localStorage.removeItem(STORAGE_KEYS.access)
    }
    if (this.user) {
      localStorage.setItem(STORAGE_KEYS.user, JSON.stringify(this.user))
    } else {
      localStorage.removeItem(STORAGE_KEYS.user)
    }
  }

  private clearStorage(): void {
    this.accessToken = null
    this.user = null
    localStorage.removeItem(STORAGE_KEYS.access)
    localStorage.removeItem(STORAGE_KEYS.user)
    localStorage.removeItem(STORAGE_KEYS.legacyRefresh)
  }

  private clearRefreshTimer(): void {
    if (this.refreshTimer) {
      clearTimeout(this.refreshTimer)
      this.refreshTimer = null
    }
  }

  private notifyCallbacks(): void {
    const authenticated = this.isAuthenticated()
    for (const cb of this.callbacks) {
      cb(authenticated)
    }
  }

  private scheduleRefresh(expiresInSeconds?: number): void {
    this.clearRefreshTimer()
    const expiryMs = (expiresInSeconds ?? 900) * 1000
    const refreshMs = Math.max(expiryMs - 120_000, 10_000) // 2 min early, min 10s
    this.refreshTimer = setTimeout(() => this.refreshTokens(), refreshMs)
  }

  async login(email: string, password: string, rememberMe?: boolean): Promise<LoginResult> {
    try {
      const res = await fetch(`${this.apiUrl}/auth/login`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password, rememberMe }),
      })
      const data = await res.json()
      if (!data.ok) {
        return {
          ok: false,
          error: data.message || data.error || AUTH_LOGIN_FALLBACK,
          errorCode: data.details?.code,
        }
      }
      this.accessToken = data.tokens.accessToken
      this.user = {
        id: data.user.id,
        email: data.user.email,
        username: data.user.username,
        orgId: data.user.orgId,
        role: data.user.role,
      }
      this.enrichUserFromToken()
      this.saveToStorage()
      if (rememberMe !== undefined) {
        localStorage.setItem(STORAGE_KEYS.rememberMe, JSON.stringify(rememberMe))
      }
      this.scheduleRefresh(data.tokens.expiresIn)
      this.notifyCallbacks()
      return { ok: true, user: this.user }
    } catch {
      return { ok: false, error: AUTH_NETWORK_ERROR }
    }
  }

  async register(email: string, password: string, username?: string): Promise<LoginResult> {
    try {
      const res = await fetch(`${this.apiUrl}/auth/register`, {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password, username }),
      })
      const data = await res.json()
      if (!data.ok) {
        return { ok: false, error: data.message || data.error || AUTH_REGISTER_FALLBACK }
      }
      // Check if tokens are present (dev mode) or email verification required
      if (data.tokens) {
        // Auto login (dev mode or SMTP disabled)
        this.accessToken = data.tokens.accessToken
        this.user = {
          id: data.user.id,
          email: data.user.email,
          username: data.user.username,
          orgId: data.user.orgId,
          role: data.user.role,
        }
        this.saveToStorage()
        this.scheduleRefresh(data.tokens.expiresIn)
        this.notifyCallbacks()
        return { ok: true, user: this.user, tokens: data.tokens }
      } else {
        // Email verification required - return user data but no tokens
        return {
          ok: true,
          user: {
            id: data.user.id,
            email: data.user.email,
            username: data.user.username,
            orgId: data.user.orgId,
            role: data.user.role,
          },
        }
      }
    } catch {
      return { ok: false, error: AUTH_NETWORK_ERROR }
    }
  }

  async refreshTokens(): Promise<boolean> {
    // Refresh token lives in an httpOnly cookie — the server reads it from the request.
    // Mutex: if a refresh is already in flight, share the same promise
    if (this.refreshPromise) return this.refreshPromise
    this.refreshPromise = this._doRefresh()
    try {
      return await this.refreshPromise
    } finally {
      this.refreshPromise = null
    }
  }

  private async _doRefresh(): Promise<boolean> {
    const maxAttempts = 2
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      try {
        const res = await fetch(`${this.apiUrl}/auth/refresh`, {
          method: 'POST',
          credentials: 'include',
        })
        const data = await res.json()
        if (!data.ok) {
          // Server explicitly rejected the refresh token — session is invalid
          this.logout()
          return false
        }
        this.accessToken = data.tokens.accessToken
        this.saveToStorage()
        this.scheduleRefresh(data.tokens.expiresIn)
        return true
      } catch {
        if (attempt < maxAttempts - 1) {
          await new Promise((r) => setTimeout(r, 2000))
          continue
        }
        // Network error after retries — don't logout (tokens may still be valid).
        // Schedule another refresh attempt so recovery is automatic when
        // connectivity is restored.
        this.scheduleRefresh(60) // retry in ~60s
        return false
      }
    }
    return false
  }

  logout(): void {
    this.clearRefreshTimer()
    // Fire and forget logout endpoint — server revokes the refresh token and clears the cookie.
    fetch(`${this.apiUrl}/auth/logout`, {
      method: 'POST',
      credentials: 'include',
      headers: this.accessToken ? { Authorization: `Bearer ${this.accessToken}` } : {},
    }).catch(() => undefined)
    this.clearStorage()
    this.notifyCallbacks()
  }

  getAccessToken(): string | null {
    return this.accessToken
  }

  getAuthHeader(): Record<string, string> {
    if (this.accessToken) {
      return { Authorization: `Bearer ${this.accessToken}` }
    }
    return {}
  }

  isAuthenticated(): boolean {
    if (!this.accessToken || !this.user) return false
    // Check if access token is expired (or will expire within 30s)
    // If expired but refresh token exists, consider unauthenticated
    // so the startup flow triggers a refresh or login
    if (this.isTokenExpired(this.accessToken, 30)) {
      return false
    }
    return true
  }

  private isTokenExpired(token: string, bufferSeconds = 0): boolean {
    try {
      const parts = token.split('.')
      if (parts.length !== 3) return true
      const payload = JSON.parse(atob(parts[1].replace(/-/g, '+').replace(/_/g, '/')))
      if (!payload.exp) return true
      return payload.exp * 1000 <= Date.now() + bufferSeconds * 1000
    } catch {
      return true
    }
  }

  getUser(): AuthUser | null {
    return this.user
  }

  getRememberMe(): boolean {
    try {
      return JSON.parse(localStorage.getItem(STORAGE_KEYS.rememberMe) ?? 'false')
    } catch {
      return false
    }
  }

  onAuthChange(callback: AuthChangeCallback): void {
    this.callbacks.push(callback)
  }

  /** Exchange SSO auth_code for tokens */
  async exchangeAuthCode(code: string): Promise<void> {
    const response = await fetch(`${this.apiUrl}/auth/sso/exchange`, {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
    })
    const data = await response.json()
    if (!data.ok) throw new Error(data.message || AUTH_SSO_EXCHANGE_FALLBACK)
    this.accessToken = data.tokens.accessToken
    this.user = {
      id: data.user.id,
      email: data.user.email,
      username: data.user.username,
      orgId: data.user.orgId,
      role: data.user.role,
    }
    this.saveToStorage()
    this.scheduleRefresh(data.tokens.expiresIn)
    this.notifyCallbacks()
  }

  /** Resend verification email */
  async resendVerification(email: string): Promise<void> {
    const response = await fetch(`${this.apiUrl}/auth/resend-verification`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email }),
    })
    const data = await response.json()
    if (!data.ok) throw new Error(data.message || AUTH_RESEND_VERIFICATION_FALLBACK)
  }

  /** Request password reset */
  async forgotPassword(email: string): Promise<void> {
    const response = await fetch(`${this.apiUrl}/auth/forgot-password`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email }),
    })
    const data = await response.json()
    if (!data.ok) throw new Error(data.message || AUTH_FORGOT_PASSWORD_FALLBACK)
  }

  /** Reset password with token */
  async resetPassword(token: string, newPassword: string): Promise<void> {
    const response = await fetch(`${this.apiUrl}/auth/reset-password`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token, newPassword }),
    })
    const data = await response.json()
    if (!data.ok) throw new Error(data.message || AUTH_RESET_PASSWORD_FALLBACK)
  }

  /** Fetch available auth providers */
  async getProviders(): Promise<Array<{ name: string; displayName: string; icon?: string }>> {
    try {
      const response = await fetch(`${this.apiUrl}/auth/providers`)
      const data = await response.json()
      return data.ok ? data.providers : []
    } catch {
      return []
    }
  }

  /**
   * Store a new access token and update org context (used by org switch).
   * The matching refresh token is set server-side as an httpOnly cookie on the same response.
   */
  updateTokens(accessToken: string, expiresInSeconds?: number, orgId?: string): void {
    if (!accessToken) return
    this.accessToken = accessToken
    if (orgId && this.user) {
      this.user = { ...this.user, orgId }
    }
    this.saveToStorage()
    this.scheduleRefresh(expiresInSeconds)
  }

  /** Helper to store user (used by exchangeAuthCode) */
  private storeUser(user: AuthUser): void {
    this.user = user
    this.saveToStorage()
  }
}
