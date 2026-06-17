/**
 * AuthPage - Full-screen login/registration page with tech-themed dark UI
 */

import type { AuthManager, LoginResult } from '@app/shared/auth/AuthManager'
import { config } from '@app/shared/config'
import { iconSuccess } from '@app/shared/ui/icons'

type AuthTab = 'login' | 'register'
type AuthRecoveryAction = 'resend-verification' | 'forgot-password' | 'reset-password'
type AuthFailure = Pick<LoginResult, 'error' | 'errorCode'>

function authFailureDetail(result: AuthFailure): {
  code: string
  detail: string
  lowerDetail: string
} {
  const code = typeof result.errorCode === 'string' ? result.errorCode.trim().toUpperCase() : ''
  const detail = typeof result.error === 'string' ? result.error.trim() : ''
  return { code, detail, lowerDetail: detail.toLowerCase() }
}

function authLoginErrorMessage(result: AuthFailure): string {
  const { code, lowerDetail } = authFailureDetail(result)
  const networkFailed =
    lowerDetail.includes('could not connect') ||
    lowerDetail.includes('network') ||
    lowerDetail.includes('failed to fetch') ||
    lowerDetail.includes('load failed')

  if (networkFailed) {
    return 'Check your connection, then try signing in again. Forge could not reach sign-in.'
  }
  if (
    code.includes('RATE') ||
    code.includes('TOO_MANY') ||
    lowerDetail.includes('too many') ||
    lowerDetail.includes('rate limit') ||
    lowerDetail.includes('429')
  ) {
    return 'Wait a few minutes, then try signing in again. Too many sign-in attempts.'
  }
  if (
    code.includes('INVALID') ||
    code.includes('UNAUTHORIZED') ||
    lowerDetail.includes('invalid credential') ||
    lowerDetail.includes('invalid email') ||
    lowerDetail.includes('incorrect password') ||
    lowerDetail.includes('wrong password') ||
    lowerDetail.includes('not found') ||
    lowerDetail.includes('unauthorized')
  ) {
    return 'Check your email and password, then try signing in again.'
  }
  if (
    lowerDetail.includes('disabled') ||
    lowerDetail.includes('locked') ||
    lowerDetail.includes('suspended') ||
    lowerDetail.includes('forbidden')
  ) {
    return 'Ask an owner or admin to check your access. This account is not allowed to sign in here.'
  }

  return 'Try signing in again in a minute. If it still fails, ask an owner or admin to check sign-in setup.'
}

function authRegisterErrorMessage(result: AuthFailure): string {
  const { code, lowerDetail } = authFailureDetail(result)
  const networkFailed =
    lowerDetail.includes('could not connect') ||
    lowerDetail.includes('network') ||
    lowerDetail.includes('failed to fetch') ||
    lowerDetail.includes('load failed')

  if (networkFailed) {
    return 'Check your connection, then create the account again. Forge could not reach account setup.'
  }
  if (
    code.includes('RATE') ||
    code.includes('TOO_MANY') ||
    lowerDetail.includes('too many') ||
    lowerDetail.includes('rate limit') ||
    lowerDetail.includes('429')
  ) {
    return 'Wait a few minutes, then create the account again. Too many account creation attempts.'
  }
  if (
    code.includes('EMAIL_ALREADY') ||
    code.includes('ALREADY_EXISTS') ||
    code.includes('CONFLICT') ||
    lowerDetail.includes('already exists') ||
    lowerDetail.includes('already registered') ||
    lowerDetail.includes('duplicate') ||
    lowerDetail.includes('conflict')
  ) {
    return 'Sign in instead, or reset the password if you cannot access it. An account may already exist for this email.'
  }
  if (
    code.includes('WEAK_PASSWORD') ||
    lowerDetail.includes('password') ||
    lowerDetail.includes('too short')
  ) {
    return 'Use a stronger password. It needs at least 12 characters and a mix of letters, numbers, and symbols.'
  }
  if (
    code.includes('INVALID_EMAIL') ||
    lowerDetail.includes('invalid email') ||
    lowerDetail.includes('email address')
  ) {
    return 'Enter a valid email address, then try creating the account again.'
  }

  return 'Check the fields, then create the account again. If it still fails, ask an owner or admin to check account setup.'
}

function authSignInErrorMessage(error: unknown): string {
  const detail =
    error instanceof Error ? error.message.trim() : typeof error === 'string' ? error.trim() : ''
  const lowerDetail = detail.toLowerCase()
  const networkFailed =
    error instanceof TypeError ||
    /^failed to fetch$/i.test(detail) ||
    lowerDetail.includes('network') ||
    lowerDetail.includes('load failed')

  if (networkFailed) {
    return 'Check your connection, then try signing in again. Forge could not reach sign-in.'
  }
  if (lowerDetail.includes('access_denied') || lowerDetail.includes('cancel')) {
    return 'Choose a sign-in option, then try again. Sign-in was cancelled.'
  }
  if (
    lowerDetail.includes('invalid_grant') ||
    lowerDetail.includes('invalid_request') ||
    lowerDetail.includes('expired') ||
    lowerDetail.includes('auth code') ||
    lowerDetail.includes('state mismatch') ||
    lowerDetail.includes('token')
  ) {
    return 'Start sign-in again from this page. This sign-in link expired or could not be verified.'
  }
  if (
    lowerDetail.includes('unauthorized') ||
    lowerDetail.includes('forbidden') ||
    lowerDetail.includes('permission')
  ) {
    return 'Ask an owner or admin to check your access. This account is not allowed to sign in here.'
  }
  if (
    lowerDetail.includes('provider') ||
    lowerDetail.includes('client') ||
    lowerDetail.includes('not configured')
  ) {
    return 'Ask an owner or admin to check sign-in setup. This sign-in option is not ready.'
  }

  return 'Choose a sign-in option and try again. If it still fails, ask an owner or admin to check sign-in setup.'
}

function authRecoveryErrorMessage(action: AuthRecoveryAction, error: unknown): string {
  const detail = error instanceof Error ? error.message.trim() : ''
  const lowerDetail = detail.toLowerCase()
  const networkFailed =
    error instanceof TypeError ||
    /^failed to fetch$/i.test(detail) ||
    lowerDetail.includes('network')

  if (networkFailed) {
    switch (action) {
      case 'resend-verification':
        return 'Check your connection, then send the verification email again. Forge could not reach email delivery.'
      case 'forgot-password':
        return 'Check your connection, then request the reset email again. Forge could not reach email delivery.'
      case 'reset-password':
        return 'Check your connection, then save the new password again. Forge could not reach password reset.'
    }
  }

  if (action === 'reset-password') {
    if (lowerDetail.includes('expired') || lowerDetail.includes('invalid')) {
      return 'Request a new reset email, then open the newest link. This reset link may have expired.'
    }
    return 'Check the password rules, then try again. Password could not be updated.'
  }

  if (action === 'forgot-password') {
    return 'Check the email address, wait a moment, then request the reset email again.'
  }

  return 'Check that this is the email you used to create the account, then send the verification email again.'
}

/** Get SSO provider icon based on provider name */
function getSsoIcon(name: string): string {
  switch (name.toLowerCase()) {
    case 'github':
      return '🐙'
    case 'google':
      return '🔵'
    case 'keycloak':
    case 'building':
      return '🏢'
    case 'gitlab':
      return '🦊'
    case 'microsoft':
    case 'azure':
      return '🪟'
    default:
      return '🔑'
  }
}

export class AuthPage {
  private container: HTMLDivElement | null = null
  private authManager: AuthManager
  private resolveAuth: (() => void) | null = null
  private currentTab: AuthTab = 'login'
  private initialResetToken: string | null = null
  private providers: Array<{ name: string; displayName: string; icon?: string }> = []

  constructor(
    authManager: AuthManager,
    initialTab: AuthTab = 'login',
    initialResetToken?: string | null
  ) {
    this.authManager = authManager
    this.currentTab = initialTab
    this.initialResetToken = initialResetToken?.trim() || null
  }

  async show(): Promise<void> {
    if (this.container) return

    // Handle URL parameters first
    const urlParams = new URLSearchParams(window.location.search)
    let signInError: unknown | null = null

    // Handle reset token
    const resetToken = this.initialResetToken ?? urlParams.get('reset_token')
    if (resetToken) {
      window.history.replaceState({}, '', window.location.pathname)
      // Fetch providers and render container first
      this.providers = await this.authManager.getProviders()
      this.container = document.createElement('div')
      this.container.id = 'auth-page'
      this.container.className = 'auth-page'
      this.container.innerHTML = this.render()
      document.body.appendChild(this.container)
      // Then show reset password view
      this.showResetPassword(resetToken)
      return
    }

    // Handle SSO callback
    const authCode = urlParams.get('auth_code')
    if (authCode) {
      window.history.replaceState({}, '', window.location.pathname) // Clean URL
      try {
        await this.authManager.exchangeAuthCode(authCode)
        this.resolveAuth?.()
        return // Auth successful, don't show login page
      } catch (error) {
        // Show error, fall through to login page
        console.error('SSO exchange failed:', error)
        signInError = error
      }
    }

    // Fetch SSO providers
    this.providers = await this.authManager.getProviders()

    this.container = document.createElement('div')
    this.container.id = 'auth-page'
    this.container.className = 'auth-page'
    this.container.innerHTML = this.render()
    document.body.appendChild(this.container)
    this.bindEvents()

    // Handle verification success after render
    const verified = urlParams.get('verified')
    if (verified === 'true') {
      window.history.replaceState({}, '', window.location.pathname)
      this.showSuccessToast('Email verified. Sign in with your email and password.')
    }

    // Handle SSO error after render
    const authError = urlParams.get('auth_error')
    if (authError) {
      window.history.replaceState({}, '', window.location.pathname)
      signInError = authError
    }
    if (signInError) {
      this.setError(authSignInErrorMessage(signInError))
    }

    // Auto-focus first input
    requestAnimationFrame(() => {
      const firstInput = this.container?.querySelector<HTMLInputElement>('.auth-input')
      firstInput?.focus()
    })
  }

  hide(): void {
    if (this.container) {
      this.container.remove()
      this.container = null
    }
  }

  waitForAuth(): Promise<void> {
    return new Promise((resolve) => {
      this.resolveAuth = resolve
    })
  }

  private render(): string {
    return `
      <div class="auth-bg">
        <div class="auth-glow auth-glow-1"></div>
        <div class="auth-glow auth-glow-2"></div>
        <div class="auth-glow auth-glow-3"></div>
      </div>
      <div class="auth-card">
        <div class="auth-header">
          <div class="auth-logo">&#9881;</div>
          <h1 class="auth-title">Wisdoverse Forge</h1>
          <p class="auth-subtitle">Team space access</p>
          <p class="auth-intro">
            Sign in to manage agents, tasks, saved work records, and team settings from one team space.
          </p>
          <p class="auth-intro auth-intro-secondary">
            New here? Create an account first. Already invited? Sign in with your email.
          </p>
        </div>
        <div class="auth-tabs">
          <button class="auth-tab active" data-tab="login">Sign in</button>
          <button class="auth-tab" data-tab="register">Create account</button>
        </div>
        <div class="auth-tab-indicator"></div>
        <div class="auth-error" id="auth-error"></div>
        <div class="auth-form-container">
          ${this.renderSsoButtons()}
          ${this.renderLoginForm()}
          ${this.renderRegisterForm()}
        </div>
        <div class="auth-footer">
          <div class="auth-footer-links">
            <a href="/terms" class="auth-footer-link" data-legal="terms">Terms of Service</a>
            <span class="auth-footer-sep">&middot;</span>
            <a href="/privacy" class="auth-footer-link" data-legal="privacy">Privacy Policy</a>
          </div>
          <div>&copy; 2026 Wisdoverse Forge</div>
        </div>
      </div>
    `
  }

  private renderSsoButtons(): string {
    if (this.providers.length === 0) return ''

    return `
      <div class="auth-sso-section">
        <div style="display: flex; flex-direction: column; gap: 10px; margin-bottom: 20px;">
          ${this.providers
            .map(
              (p) => `
            <button class="sso-btn" data-provider="${p.name}" style="
              width: 100%; padding: 12px; border-radius: 8px;
              background: rgba(42, 42, 74, 0.8); border: 1px solid rgba(58, 58, 90, 0.8); color: #e0e0e0;
              cursor: pointer; font-size: 14px; display: flex; align-items: center;
              justify-content: center; gap: 8px; transition: all 0.2s;
            ">
              <span style="font-size: 18px;">${getSsoIcon(p.icon || p.name)}</span>
              <span>Continue with ${p.displayName}</span>
            </button>
          `
            )
            .join('')}
        </div>
        <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 20px;">
          <div style="flex: 1; height: 1px; background: rgba(58, 58, 90, 0.5);"></div>
          <span style="color: #64748b; font-size: 13px;">or use your email address</span>
          <div style="flex: 1; height: 1px; background: rgba(58, 58, 90, 0.5);"></div>
        </div>
      </div>
    `
  }

  private renderLoginForm(): string {
    return `
      <form class="auth-form" id="login-form">
        <p class="auth-form-note">
          Use the email your workspace admin invited. After sign in, you will land on your task board.
        </p>
        <div class="auth-field">
          <label class="auth-label" for="login-email">Email address</label>
          <input class="auth-input" id="login-email" type="email" placeholder="name@example.com" autocomplete="email" required>
        </div>
        <div class="auth-field">
          <label class="auth-label" for="login-password">Password</label>
          <div class="auth-password-wrap">
            <input class="auth-input" id="login-password" type="password" placeholder="Your account password" autocomplete="current-password" required>
            <button type="button" class="auth-password-toggle" data-target="login-password" aria-label="Show or hide password">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            </button>
          </div>
        </div>
        <div class="auth-remember">
          <label class="auth-remember-label">
            <input type="checkbox" id="login-remember" class="auth-remember-check">
            <span class="auth-remember-text">Keep me signed in on this device</span>
          </label>
        </div>
        <button class="auth-submit" type="submit" id="login-submit">
          <span class="auth-submit-text">Sign in</span>
          <span class="auth-submit-spinner" hidden></span>
        </button>
        <a href="#" class="auth-back-link" id="forgot-password-link">I cannot access my password</a>
      </form>
    `
  }

  private renderRegisterForm(): string {
    return `
      <form class="auth-form" id="register-form" style="display:none">
        <p class="auth-form-note">
          Create your first team space account. You can invite teammates and connect agents after you get in.
        </p>
        <div class="auth-field">
          <label class="auth-label" for="register-email">Email address</label>
          <input class="auth-input" id="register-email" type="email" placeholder="name@example.com" autocomplete="email" required>
          <span class="auth-hint">We use this for verification, password reset, and team space alerts.</span>
        </div>
        <div class="auth-field">
          <label class="auth-label" for="register-username">Display name <span class="auth-optional">(optional)</span></label>
          <input class="auth-input" id="register-username" type="text" placeholder="What teammates should see" autocomplete="username">
        </div>
        <div class="auth-field">
          <label class="auth-label" for="register-password">Create a password</label>
          <div class="auth-password-wrap">
            <input class="auth-input" id="register-password" type="password" placeholder="At least 12 characters" autocomplete="new-password" required>
            <button type="button" class="auth-password-toggle" data-target="register-password" aria-label="Show or hide password">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            </button>
          </div>
          <div class="auth-strength">
            <div class="auth-strength-bar"><div class="auth-strength-fill"></div></div>
            <span class="auth-strength-text"></span>
          </div>
          <div class="auth-password-rules">
            <span class="auth-rule" data-rule="length">12 characters</span>
            <span class="auth-rule" data-rule="upper">uppercase letter</span>
            <span class="auth-rule" data-rule="lower">lowercase letter</span>
            <span class="auth-rule" data-rule="number">number</span>
            <span class="auth-rule" data-rule="special">symbol</span>
          </div>
        </div>
        <button class="auth-submit" type="submit" id="register-submit">
          <span class="auth-submit-text">Create account and continue</span>
          <span class="auth-submit-spinner" hidden></span>
        </button>
      </form>
    `
  }

  private bindEvents(): void {
    if (!this.container) return

    // Tab switching
    const tabs = this.container.querySelectorAll<HTMLButtonElement>('.auth-tab')
    tabs.forEach((tab) => {
      tab.addEventListener('click', () => {
        const target = tab.dataset.tab as AuthTab
        this.switchTab(target)
      })
    })

    // SSO buttons
    const ssoButtons = this.container.querySelectorAll<HTMLButtonElement>('.sso-btn')
    ssoButtons.forEach((btn) => {
      btn.addEventListener('click', () => {
        const provider = btn.dataset.provider
        if (provider) {
          window.location.href = `${config.apiUrl}/auth/sso/${provider}`
        }
      })
      // Hover effects
      btn.addEventListener('mouseenter', () => {
        btn.style.background = 'rgba(58, 58, 90, 1)'
        btn.style.borderColor = 'rgba(99, 102, 241, 0.4)'
      })
      btn.addEventListener('mouseleave', () => {
        btn.style.background = 'rgba(42, 42, 74, 0.8)'
        btn.style.borderColor = 'rgba(58, 58, 90, 0.8)'
      })
    })

    // Login form
    const loginForm = this.container.querySelector<HTMLFormElement>('#login-form')
    loginForm?.addEventListener('submit', (e) => {
      e.preventDefault()
      void this.handleLogin()
    })

    // Register form
    const registerForm = this.container.querySelector<HTMLFormElement>('#register-form')
    registerForm?.addEventListener('submit', (e) => {
      e.preventDefault()
      void this.handleRegister()
    })

    // Forgot password link
    const forgotLink = this.container.querySelector<HTMLAnchorElement>('#forgot-password-link')
    forgotLink?.addEventListener('click', (e) => {
      e.preventDefault()
      this.showForgotPassword()
    })

    // Password toggles
    this.bindPasswordToggles(this.container)

    // Password strength meter
    const passwordInput = this.container.querySelector<HTMLInputElement>('#register-password')
    passwordInput?.addEventListener('input', () => {
      this.updatePasswordStrength(passwordInput.value)
    })

    // Restore Remember Me checkbox from previous preference
    const rememberCheck = this.container.querySelector<HTMLInputElement>('#login-remember')
    if (rememberCheck) {
      rememberCheck.checked = this.authManager.getRememberMe()
    }

    // Legal page links
    const legalLinks = this.container.querySelectorAll<HTMLAnchorElement>('.auth-footer-link')
    legalLinks.forEach((link) => {
      link.addEventListener('click', (e) => {
        e.preventDefault()
        const tab = link.dataset.legal as 'terms' | 'privacy'
        // Dynamically import LegalPage to avoid circular deps
        void import('@app/shared/ui/legal/LegalPage').then(({ LegalPage }) => {
          const legalPage = new LegalPage()
          legalPage.show(tab)
        })
      })
    })
  }

  private switchTab(tab: AuthTab): void {
    this.currentTab = tab
    if (!this.container) return

    // Update tab buttons
    const tabs = this.container.querySelectorAll<HTMLButtonElement>('.auth-tab')
    tabs.forEach((t) => {
      t.classList.toggle('active', t.dataset.tab === tab)
    })

    // Update tab indicator
    const indicator = this.container.querySelector<HTMLDivElement>('.auth-tab-indicator')
    if (indicator) {
      indicator.style.transform = tab === 'login' ? 'translateX(0)' : 'translateX(100%)'
    }

    // Show/hide forms
    const loginForm = this.container.querySelector<HTMLFormElement>('#login-form')
    const registerForm = this.container.querySelector<HTMLFormElement>('#register-form')
    if (loginForm) loginForm.style.display = tab === 'login' ? '' : 'none'
    if (registerForm) registerForm.style.display = tab === 'register' ? '' : 'none'

    // Clear error
    this.setError('')

    // Focus first input of active form
    requestAnimationFrame(() => {
      const form = tab === 'login' ? loginForm : registerForm
      const firstInput = form?.querySelector<HTMLInputElement>('.auth-input')
      firstInput?.focus()
    })
  }

  private async handleLogin(): Promise<void> {
    const email = this.getInput('login-email')
    const password = this.getInput('login-password')
    const rememberMe =
      this.container?.querySelector<HTMLInputElement>('#login-remember')?.checked ?? false
    if (!email || !password) {
      this.setError('Enter your email address and password to sign in.')
      return
    }
    this.setLoading('login-submit', true)
    this.setError('')
    const result = await this.authManager.login(email, password, rememberMe)
    this.setLoading('login-submit', false)
    if (result.ok) {
      this.resolveAuth?.()
    } else if (result.errorCode === 'EMAIL_NOT_VERIFIED') {
      this.setError('')
      this.showVerificationBanner(email)
    } else {
      this.setError(authLoginErrorMessage(result))
      this.shakeCard()
    }
  }

  private async handleRegister(): Promise<void> {
    const email = this.getInput('register-email')
    const password = this.getInput('register-password')
    const username = this.getInput('register-username') || undefined
    if (!email || !password) {
      this.setError('Enter an email address and password to create your account.')
      return
    }
    this.setLoading('register-submit', true)
    this.setError('')
    const result = await this.authManager.register(email, password, username)
    this.setLoading('register-submit', false)
    if (result.ok) {
      if (result.tokens) {
        // Auto logged in (dev mode or SMTP disabled)
        this.resolveAuth?.()
      } else {
        // Email verification required
        this.showVerificationPending(email)
      }
    } else {
      this.setError(authRegisterErrorMessage(result))
      this.shakeCard()
    }
  }

  private getInput(id: string): string {
    const input = this.container?.querySelector<HTMLInputElement>(`#${id}`)
    return input?.value.trim() ?? ''
  }

  private setError(message: string): void {
    const el = this.container?.querySelector<HTMLDivElement>('#auth-error')
    if (el) {
      el.textContent = message
      el.style.display = message ? '' : 'none'
    }
    // Clear verification banner when showing a different error
    const banner = this.container?.querySelector<HTMLDivElement>('#auth-verify-banner')
    if (banner) banner.remove()
  }

  private showVerificationBanner(email: string): void {
    // Remove existing banner if any
    this.container?.querySelector('#auth-verify-banner')?.remove()

    const banner = document.createElement('div')
    banner.id = 'auth-verify-banner'
    banner.className = 'auth-verify-banner'
    banner.innerHTML = `
      <div class="auth-verify-banner-icon">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
          <polyline points="22,6 12,13 2,6"/>
        </svg>
      </div>
      <div class="auth-verify-banner-content">
        <div class="auth-verify-banner-title">Check your email first</div>
        <div class="auth-verify-banner-text">We sent a verification link to <strong id="verify-email-display"></strong>. Open it, then come back and sign in.</div>
        <button type="button" class="auth-verify-banner-resend" id="verify-resend-btn">Send verification email again</button>
        <div id="verify-resend-error" class="auth-error" style="display:none; margin:8px 0 0;"></div>
      </div>
    `

    // Set email via textContent to prevent XSS
    const emailDisplay = banner.querySelector('#verify-email-display')
    if (emailDisplay) emailDisplay.textContent = email

    // Insert before the form container
    const formContainer = this.container?.querySelector('.auth-form-container')
    formContainer?.parentNode?.insertBefore(banner, formContainer)

    // Wire up resend button
    const resendBtn = banner.querySelector<HTMLButtonElement>('#verify-resend-btn')
    const resendError = banner.querySelector<HTMLDivElement>('#verify-resend-error')
    resendBtn?.addEventListener('click', async () => {
      if (!resendBtn || resendBtn.disabled) return
      resendBtn.disabled = true
      resendBtn.textContent = 'Sending...'
      if (resendError) {
        resendError.textContent = ''
        resendError.style.display = 'none'
      }
      try {
        await this.authManager.resendVerification(email)
        resendBtn.textContent = 'Sent. Check your inbox.'
        resendBtn.classList.add('sent')
        setTimeout(() => {
          resendBtn.disabled = false
          resendBtn.textContent = 'Send verification email again'
          resendBtn.classList.remove('sent')
        }, 60000)
      } catch (err) {
        console.error('[AuthPage] Failed to resend verification email:', err)
        resendBtn.disabled = false
        resendBtn.textContent = 'Send verification email again'
        if (resendError) {
          resendError.textContent = authRecoveryErrorMessage('resend-verification', err)
          resendError.style.display = ''
        }
      }
    })
  }

  private setLoading(buttonId: string, loading: boolean): void {
    const button = this.container?.querySelector<HTMLButtonElement>(`#${buttonId}`)
    if (!button) return
    button.disabled = loading
    const text = button.querySelector<HTMLSpanElement>('.auth-submit-text')
    const spinner = button.querySelector<HTMLSpanElement>('.auth-submit-spinner')
    if (text) text.hidden = loading
    if (spinner) spinner.hidden = !loading
  }

  private shakeCard(): void {
    const card = this.container?.querySelector<HTMLDivElement>('.auth-card')
    if (card) {
      card.classList.add('shake')
      setTimeout(() => card.classList.remove('shake'), 500)
    }
  }

  private bindPasswordToggles(root: HTMLElement): void {
    const toggles = root.querySelectorAll<HTMLButtonElement>('.auth-password-toggle')
    toggles.forEach((toggle) => {
      toggle.addEventListener('click', () => {
        const targetId = toggle.dataset.target
        if (!targetId) return
        const input = root.querySelector<HTMLInputElement>(`#${targetId}`)
        if (input) {
          input.type = input.type === 'password' ? 'text' : 'password'
          toggle.classList.toggle('active')
        }
      })
    })
  }

  private updatePasswordStrength(password: string, scope?: HTMLElement): void {
    const root = scope || this.container
    if (!root) return

    const rules: Record<string, boolean> = {
      length: password.length >= 12,
      upper: /[A-Z]/.test(password),
      lower: /[a-z]/.test(password),
      number: /[0-9]/.test(password),
      special: /[!@#$%^&*()_+\-=[\]{};':"\\|,.<>/?`~]/.test(password),
    }

    // Update rule indicators
    for (const [key, met] of Object.entries(rules)) {
      const el = root.querySelector<HTMLSpanElement>(`.auth-rule[data-rule="${key}"]`)
      if (el) {
        el.classList.toggle('met', met)
      }
    }

    // Calculate strength
    const metCount = Object.values(rules).filter(Boolean).length
    const fill = root.querySelector<HTMLDivElement>('.auth-strength-fill')
    const text = root.querySelector<HTMLSpanElement>('.auth-strength-text')
    const strengthContainer = root.querySelector<HTMLDivElement>('.auth-strength')

    if (!fill || !text || !strengthContainer) return

    if (password.length === 0) {
      strengthContainer.style.display = 'none'
      return
    }
    strengthContainer.style.display = ''

    const pct = (metCount / 5) * 100
    fill.style.width = `${pct}%`

    if (metCount <= 2) {
      fill.className = 'auth-strength-fill weak'
      text.textContent = 'Keep adding details'
    } else if (metCount <= 3) {
      fill.className = 'auth-strength-fill fair'
      text.textContent = 'Almost there'
    } else if (metCount === 4) {
      fill.className = 'auth-strength-fill good'
      text.textContent = 'Good'
    } else {
      fill.className = 'auth-strength-fill strong'
      text.textContent = 'Strong'
    }
  }

  private showVerificationPending(email: string): void {
    const container = this.container?.querySelector('.auth-form-container') || this.container
    if (!container) return
    // Build the static markup with `innerHTML`, then inject the user-controlled
    // `email` via `textContent` so a registration-supplied address that
    // contains HTML (e.g. `<script>...</script>@example.com`) cannot break out
    // of the placeholder. Closes the `js/xss-through-dom` finding CodeQL
    // raised against the previous template-literal interpolation.
    container.innerHTML = `
      <div style="text-align: center; padding: 40px 20px;">
        <div style="font-size: 48px; margin-bottom: 16px;">📧</div>
        <h2 style="color: #818cf8; margin: 0 0 16px; font-size: 20px; font-weight: 600;">Check your email</h2>
        <p style="color: #94a3b8; margin: 0 0 24px; line-height: 1.6; font-size: 14px;">
          We sent a verification link to<br/>
          <strong id="verify-email-target" style="color: #e2e8f0;"></strong>
        </p>
        <p style="color: #64748b; font-size: 13px; margin: 0 0 24px;">
          Open that email to finish creating your account. You can sign in after verification.
        </p>
        <button id="resend-btn" style="
          background: transparent; border: 1px solid rgba(58, 58, 90, 0.8); color: #818cf8;
          padding: 10px 24px; border-radius: 8px; cursor: pointer; font-size: 14px;
          margin-bottom: 16px; transition: all 0.2s; font-weight: 500;
        ">Send verification email again</button>
        <p id="resend-error" style="display:none; color:#fca5a5; font-size:13px; margin:0 0 16px;"></p>
        <br/>
        <a href="#" id="back-to-login" style="color: #64748b; font-size: 13px; text-decoration: none; transition: color 0.2s;">Back to sign in</a>
      </div>
    `
    const emailTarget = container.querySelector('#verify-email-target')
    if (emailTarget) emailTarget.textContent = email
    // Add event listeners
    const resendBtn = container.querySelector('#resend-btn') as HTMLButtonElement
    const resendError = container.querySelector('#resend-error') as HTMLElement | null
    resendBtn?.addEventListener('click', async () => {
      resendBtn.disabled = true
      resendBtn.textContent = 'Sending...'
      if (resendError) {
        resendError.textContent = ''
        resendError.style.display = 'none'
      }
      try {
        await this.authManager.resendVerification(email)
        resendBtn.textContent = 'Sent. Try again in 60s'
        setTimeout(() => {
          resendBtn.disabled = false
          resendBtn.textContent = 'Send verification email again'
        }, 60000)
      } catch (error) {
        resendBtn.disabled = false
        resendBtn.textContent = 'Send verification email again'
        if (resendError) {
          resendError.textContent = authRecoveryErrorMessage('resend-verification', error)
          resendError.style.display = ''
        }
      }
    })
    resendBtn?.addEventListener('mouseenter', () => {
      if (!resendBtn.disabled) {
        resendBtn.style.borderColor = 'rgba(99, 102, 241, 0.5)'
        resendBtn.style.background = 'rgba(99, 102, 241, 0.05)'
      }
    })
    resendBtn?.addEventListener('mouseleave', () => {
      resendBtn.style.borderColor = 'rgba(58, 58, 90, 0.8)'
      resendBtn.style.background = 'transparent'
    })

    const backLink = container.querySelector('#back-to-login') as HTMLAnchorElement
    backLink?.addEventListener('click', (e) => {
      e.preventDefault()
      this.switchToLogin()
    })
  }

  private getFormContainer(): HTMLElement | null {
    return (this.container?.querySelector('.auth-form-container') as HTMLElement) || this.container
  }

  private showForgotPassword(): void {
    const container = this.getFormContainer()
    if (!container) return

    // Hide tabs
    const tabs = this.container?.querySelector<HTMLDivElement>('.auth-tabs')
    if (tabs) tabs.style.display = 'none'

    container.innerHTML = `
      <form class="auth-form" id="forgot-form">
        <h2 class="auth-form-heading">Reset your password</h2>
        <p class="auth-form-desc">Enter your account email. If it matches an account, we will email a reset link.</p>
        <div class="auth-field">
          <label class="auth-label" for="forgot-email">Email address</label>
          <input class="auth-input" id="forgot-email" type="email" placeholder="name@example.com" autocomplete="email" required>
        </div>
        <div class="auth-error" id="forgot-error"></div>
        <button class="auth-submit" type="submit" id="forgot-submit">
          <span class="auth-submit-text">Email me a reset link</span>
          <span class="auth-submit-spinner" hidden></span>
        </button>
        <a href="#" class="auth-back-link" id="back-to-login-from-forgot">&larr; Back to sign in</a>
      </form>
      <div class="auth-form" id="forgot-success" style="display:none">
        <div class="auth-form-icon">&#x1F4E7;</div>
        <h2 class="auth-form-heading">Check your inbox</h2>
        <p class="auth-form-desc">If an account exists for <strong class="auth-email-masked" id="forgot-email-masked"></strong>, you will receive a reset link.</p>
        <p class="auth-check-spam">Open the email and follow the link. Check spam if it is not in your inbox.</p>
        <p class="auth-cooldown" id="forgot-cooldown"></p>
        <a href="#" class="auth-back-link" id="back-to-login-from-success">&larr; Back to sign in</a>
      </div>
    `

    const form = container.querySelector('#forgot-form') as HTMLFormElement
    const errorDiv = container.querySelector('#forgot-error') as HTMLElement
    const successDiv = container.querySelector('#forgot-success') as HTMLElement
    let cooldownTimer: ReturnType<typeof setInterval> | null = null

    form.addEventListener('submit', async (e) => {
      e.preventDefault()
      const emailInput = container.querySelector('#forgot-email') as HTMLInputElement
      const email = emailInput.value.trim()
      if (!email) return

      this.setLoading('forgot-submit', true)
      errorDiv.style.display = 'none'

      try {
        await this.authManager.forgotPassword(email)
        form.style.display = 'none'
        successDiv.style.display = ''

        // Mask email (u***@example.com)
        const maskedEl = container.querySelector('#forgot-email-masked')
        if (maskedEl) {
          const [local, domain] = email.split('@')
          maskedEl.textContent = `${local[0]}${'*'.repeat(Math.max(local.length - 1, 2))}@${domain}`
        }

        // Start 60s cooldown
        let remaining = 60
        const cooldownEl = container.querySelector('#forgot-cooldown') as HTMLElement
        cooldownEl.textContent = `You can request another email in ${remaining}s`
        cooldownTimer = setInterval(() => {
          remaining--
          if (remaining <= 0) {
            if (cooldownTimer) clearInterval(cooldownTimer)
            cooldownEl.textContent = ''
          } else {
            cooldownEl.textContent = `You can request another email in ${remaining}s`
          }
        }, 1000)
      } catch (error) {
        this.setLoading('forgot-submit', false)
        errorDiv.textContent = authRecoveryErrorMessage('forgot-password', error)
        errorDiv.style.display = ''
      }
    })

    const backHandler = (e: Event) => {
      e.preventDefault()
      if (cooldownTimer) clearInterval(cooldownTimer)
      this.switchToLogin()
    }
    container.querySelector('#back-to-login-from-forgot')?.addEventListener('click', backHandler)
    container.querySelector('#back-to-login-from-success')?.addEventListener('click', backHandler)

    // Focus email input
    requestAnimationFrame(() => {
      const emailInput = container.querySelector<HTMLInputElement>('#forgot-email')
      emailInput?.focus()
    })
  }

  private showResetPassword(token: string): void {
    const container = this.getFormContainer()
    if (!container) return

    // Hide tabs
    const tabs = this.container?.querySelector<HTMLDivElement>('.auth-tabs')
    if (tabs) tabs.style.display = 'none'

    container.innerHTML = `
      <form class="auth-form" id="reset-form">
        <h2 class="auth-form-heading">Choose a new password</h2>
        <p class="auth-form-desc">This only changes your Wisdoverse Forge account password.</p>
        <div class="auth-field">
          <label class="auth-label" for="reset-password">New password</label>
          <div class="auth-password-wrap">
            <input class="auth-input" id="reset-password" type="password" placeholder="At least 12 characters" autocomplete="new-password" required>
            <button type="button" class="auth-password-toggle" data-target="reset-password" aria-label="Show or hide password">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            </button>
          </div>
          <div class="auth-strength">
            <div class="auth-strength-bar"><div class="auth-strength-fill"></div></div>
            <span class="auth-strength-text"></span>
          </div>
          <div class="auth-password-rules">
            <span class="auth-rule" data-rule="length">12 characters</span>
            <span class="auth-rule" data-rule="upper">uppercase letter</span>
            <span class="auth-rule" data-rule="lower">lowercase letter</span>
            <span class="auth-rule" data-rule="number">number</span>
            <span class="auth-rule" data-rule="special">symbol</span>
          </div>
        </div>
        <div class="auth-field">
          <label class="auth-label" for="reset-confirm">Confirm new password</label>
          <div class="auth-password-wrap">
            <input class="auth-input" id="reset-confirm" type="password" placeholder="Type the same password again" autocomplete="new-password" required>
            <button type="button" class="auth-password-toggle" data-target="reset-confirm" aria-label="Show or hide password">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            </button>
          </div>
        </div>
        <div class="auth-error" id="reset-error"></div>
        <button class="auth-submit" type="submit" id="reset-submit">
          <span class="auth-submit-text">Save new password</span>
          <span class="auth-submit-spinner" hidden></span>
        </button>
      </form>
      <div class="auth-form" id="reset-success" style="display:none">
        <div class="auth-form-icon">${iconSuccess}</div>
        <h2 class="auth-form-heading">Password updated</h2>
        <p class="auth-form-desc">Return to sign in with the new password in <span class="auth-countdown" id="reset-countdown">5</span>s.</p>
        <button class="auth-submit" id="go-to-login">Sign in now</button>
      </div>
    `

    const form = container.querySelector('#reset-form') as HTMLFormElement
    const errorDiv = container.querySelector('#reset-error') as HTMLElement
    const successDiv = container.querySelector('#reset-success') as HTMLElement

    // Password toggles + strength meter
    this.bindPasswordToggles(container)
    const passwordInput = container.querySelector<HTMLInputElement>('#reset-password')
    passwordInput?.addEventListener('input', () => {
      this.updatePasswordStrength(passwordInput.value, container)
    })

    form.addEventListener('submit', async (e) => {
      e.preventDefault()
      const password = (container.querySelector('#reset-password') as HTMLInputElement).value
      const confirm = (container.querySelector('#reset-confirm') as HTMLInputElement).value

      errorDiv.style.display = 'none'

      if (password !== confirm) {
        errorDiv.textContent =
          'The two passwords do not match. Re-enter both fields, then try again.'
        errorDiv.style.display = ''
        this.shakeCard()
        return
      }
      if (password.length < 12) {
        errorDiv.textContent =
          'Use at least 12 characters for the new password. Add a few more characters, then try again.'
        errorDiv.style.display = ''
        this.shakeCard()
        return
      }

      this.setLoading('reset-submit', true)
      try {
        await this.authManager.resetPassword(token, password)
        form.style.display = 'none'
        successDiv.style.display = ''

        // Auto-redirect countdown
        let remaining = 5
        const countdownEl = container.querySelector('#reset-countdown') as HTMLElement
        const navigateToLogin = () => {
          clearInterval(countdownTimer)
          window.history.replaceState({}, '', window.location.pathname)
          this.switchToLogin()
        }
        const countdownTimer = setInterval(() => {
          remaining--
          if (remaining <= 0) {
            navigateToLogin()
          } else {
            countdownEl.textContent = String(remaining)
          }
        }, 1000)

        container.querySelector('#go-to-login')?.addEventListener('click', (e) => {
          e.preventDefault()
          navigateToLogin()
        })
      } catch (error) {
        errorDiv.textContent = authRecoveryErrorMessage('reset-password', error)
        errorDiv.style.display = ''
        this.setLoading('reset-submit', false)
        this.shakeCard()
      }
    })

    // Focus password input
    requestAnimationFrame(() => {
      passwordInput?.focus()
    })
  }

  private switchToLogin(): void {
    if (!this.container) return

    // Restore tabs
    const tabs = this.container.querySelector<HTMLDivElement>('.auth-tabs')
    if (tabs) tabs.style.display = ''

    // Restore form container content
    const formContainer = this.container.querySelector('.auth-form-container')
    if (formContainer) {
      formContainer.innerHTML = `
        ${this.renderSsoButtons()}
        ${this.renderLoginForm()}
        ${this.renderRegisterForm()}
      `
    }

    // Clear error
    this.setError('')

    // Re-bind events (tabs are already bound, we need form-level events)
    this.bindEvents()

    // Switch to login tab
    this.switchTab('login')
  }

  private showSuccessToast(message: string): void {
    const toast = document.createElement('div')
    toast.style.cssText = `
      position: fixed; top: 20px; right: 20px; z-index: 10001;
      background: rgba(34, 197, 94, 0.15); border: 1px solid rgba(34, 197, 94, 0.3);
      color: #4ade80; padding: 12px 20px; border-radius: 8px;
      font-size: 14px; box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
      animation: slideInRight 0.3s ease-out;
    `
    toast.textContent = message
    document.body.appendChild(toast)
    setTimeout(() => {
      toast.style.animation = 'slideOutRight 0.3s ease-in'
      setTimeout(() => toast.remove(), 300)
    }, 4000)
  }
}
