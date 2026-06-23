import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

import { AuthPage } from '@app/features/auth'
import type { AuthManager } from '@app/shared/auth/AuthManager'

function createAuthManager(overrides: Partial<AuthManager> = {}): AuthManager {
  return {
    getProviders: vi.fn().mockResolvedValue([]),
    exchangeAuthCode: vi.fn().mockResolvedValue(undefined),
    getRememberMe: vi.fn().mockReturnValue(false),
    login: vi.fn().mockResolvedValue({ ok: false }),
    register: vi.fn().mockResolvedValue({ ok: false }),
    resendVerification: vi.fn().mockResolvedValue(undefined),
    forgotPassword: vi.fn().mockResolvedValue(undefined),
    resetPassword: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as AuthManager
}

function bodyText(): string {
  return document.body.textContent ?? ''
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

async function flushAsyncWork(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
}

beforeEach(() => {
  document.body.innerHTML = ''
  window.history.replaceState({}, '', '/login')
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    callback(0)
    return 0
  })
})

afterEach(() => {
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
})

describe('AuthPage beginner guidance', () => {
  test('renders clear first-run orientation on the sign-in screen', async () => {
    const page = new AuthPage(createAuthManager())

    await page.show()

    expect(bodyText()).toContain('Team space access')
    expect(bodyText()).toContain('Sign in to manage agents, tasks, saved work, and team settings')
    expect(bodyText()).not.toContain('saved work records')
    expect(bodyText()).toContain('from one team space')
    expect(bodyText()).toContain('Use the email address from your invitation')
    expect(bodyText()).not.toContain('workspace admin invited')
    expect(bodyText()).toContain('New here? Create an account first.')
    expect(bodyText()).not.toContain('evidence')
    expect(bodyText()).not.toContain('Team workspace access')
    expect(document.querySelector('.auth-logo')?.textContent).toBe('WF')
    expect(bodyText()).not.toContain('\u2699')
    expect(document.querySelector('#login-submit')?.textContent).toContain('Sign in')
    expect(document.querySelector('#forgot-password-link')?.textContent).toContain(
      'I cannot access my password'
    )
  })

  test('explains sign-in options and account creation in beginner language', async () => {
    const page = new AuthPage(
      createAuthManager({
        getProviders: vi.fn().mockResolvedValue([{ name: 'github', displayName: 'GitHub' }]),
      })
    )

    await page.show()
    const registerTab = document.querySelector<HTMLButtonElement>('[data-tab="register"]')
    registerTab?.click()

    expect(bodyText()).toContain('Continue with GitHub')
    expect(bodyText()).toContain('or use your email address')
    expect(document.querySelector<HTMLButtonElement>('.sso-btn')?.textContent?.trim()).toBe(
      'Continue with GitHub'
    )
    expect(bodyText()).not.toMatch(/[\u{1F300}-\u{1FAFF}]/u)
    expect(document.querySelector<HTMLFormElement>('#register-form')?.style.display).toBe('')
    expect(document.querySelector('#register-form')?.textContent).toContain(
      'Create your first team space account.'
    )
    expect(document.querySelector('#register-form')?.textContent).toContain('team space alerts')
    expect(document.querySelector('#register-form')?.textContent).toContain('Confirm password')
    expect(document.querySelector('#register-form')?.textContent).toContain(
      'Type it again so you know what to use next time you sign in.'
    )
    expect(document.querySelector('#register-form')?.textContent).not.toContain('workspace account')
    expect(document.querySelector('#register-submit')?.textContent).toContain(
      'Create account and continue'
    )
  })

  test('names the Sign in button when required sign-in fields are missing', async () => {
    const login = vi.fn().mockResolvedValue({ ok: true })
    const page = new AuthPage(createAuthManager({ login }))

    await page.show()
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'Enter your email address and password, then choose Sign in.'
    )
    expect(bodyText()).not.toContain('Enter your email address and password to sign in.')
    expect(login).not.toHaveBeenCalled()
  })

  test('names the Create account button when required account fields are missing', async () => {
    const register = vi.fn().mockResolvedValue({ ok: true })
    const page = new AuthPage(createAuthManager({ register }))

    await page.show()
    document.querySelector<HTMLButtonElement>('[data-tab="register"]')?.click()
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'Enter an email address and type the new password twice, then choose Create account and continue.'
    )
    expect(bodyText()).not.toContain('type the new password twice to create your account')
    expect(register).not.toHaveBeenCalled()
  })

  test('turns sign-in URL errors into beginner recovery guidance', async () => {
    window.history.replaceState(
      {},
      '',
      '/login?auth_error=invalid_grant%3A%20oauth%20token%20expired'
    )
    const page = new AuthPage(createAuthManager())

    await page.show()

    expect(bodyText()).toContain(
      'Start sign-in again from this page. This sign-in link expired or could not be verified.'
    )
    expect(bodyText()).not.toContain('invalid_grant')
    expect(bodyText()).not.toContain('oauth token expired')
    expect(window.location.search).toBe('')
  })

  test('turns cancelled sign-in into a concrete sign-in choice', async () => {
    window.history.replaceState({}, '', '/login?auth_error=access_denied%3A%20cancelled')
    const page = new AuthPage(createAuthManager())

    await page.show()

    expect(bodyText()).toContain(
      'Choose Password sign-in or a listed sign-in button, then start sign-in again. Sign-in was cancelled.'
    )
    expect(bodyText()).not.toContain('access_denied')
    expect(bodyText()).not.toContain('then try again')
    expect(window.location.search).toBe('')
  })

  test('turns unknown sign-in URL errors into a concrete sign-in choice', async () => {
    window.history.replaceState({}, '', '/login?auth_error=unexpected_oops')
    const page = new AuthPage(createAuthManager())

    await page.show()

    expect(bodyText()).toContain(
      'Choose Password sign-in or a listed sign-in button, then start sign-in again. If it still fails, ask an owner or admin to check the sign-in option for this page.'
    )
    expect(bodyText()).not.toContain('unexpected_oops')
    expect(bodyText()).not.toContain('Choose a sign-in option and try again')
    expect(window.location.search).toBe('')
  })

  test('shows a recovery step when provider sign-in callback fails', async () => {
    window.history.replaceState({}, '', '/login?auth_code=callback-code')
    const exchangeAuthCode = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'))
    const page = new AuthPage(createAuthManager({ exchangeAuthCode }))

    await page.show()

    expect(exchangeAuthCode).toHaveBeenCalledWith('callback-code')
    expect(bodyText()).toContain(
      'Check your connection, then choose Sign in again. Forge could not reach sign-in.'
    )
    expect(bodyText()).not.toContain('try signing in')
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
    expect(window.location.search).toBe('')
  })

  test('explains sign-in setup failures without provider wording', async () => {
    window.history.replaceState({}, '', '/login?auth_error=provider_not_configured')
    const page = new AuthPage(createAuthManager())

    await page.show()

    expect(bodyText()).toContain(
      'Ask an owner or admin to check the sign-in option for this page. This sign-in option is not ready.'
    )
    expect(bodyText()).not.toContain('sign-in setup')
    expect(bodyText()).not.toContain('provider_not_configured')
    expect(bodyText()).not.toContain('Sign-in provider')
    expect(window.location.search).toBe('')
  })

  test('keeps email sign-in backend failures beginner-safe', async () => {
    const page = new AuthPage(
      createAuthManager({
        login: vi.fn().mockResolvedValue({
          ok: false,
          error: 'database unavailable: connection refused HTTP 500',
        }),
      })
    )

    await page.show()
    const emailInput = document.querySelector<HTMLInputElement>('#login-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#login-password')
    if (emailInput) emailInput.value = 'operator@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Wait a minute, then choose Sign in again. If it still fails, ask an owner or admin to check the sign-in option for this page.'
    )
    expect(bodyText()).not.toContain('Try signing in again')
    expect(bodyText()).not.toContain('sign-in setup')
    expect(bodyText()).not.toContain('database unavailable')
    expect(bodyText()).not.toContain('HTTP 500')
  })

  test('names the Sign in button for invalid email sign-in credentials', async () => {
    const page = new AuthPage(
      createAuthManager({
        login: vi.fn().mockResolvedValue({
          ok: false,
          errorCode: 'INVALID_CREDENTIALS',
          error: 'invalid credentials',
        }),
      })
    )

    await page.show()
    const emailInput = document.querySelector<HTMLInputElement>('#login-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#login-password')
    if (emailInput) emailInput.value = 'operator@example.com'
    if (passwordInput) passwordInput.value = 'WrongPassword123!'
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain('Check your email and password, then choose Sign in again.')
    expect(bodyText()).not.toContain('try signing in')
    expect(bodyText()).not.toContain('invalid credentials')
  })

  test('explains email sign-in connection failures without raw network text', async () => {
    const page = new AuthPage(
      createAuthManager({
        login: vi.fn().mockResolvedValue({
          ok: false,
          error: 'Network error',
        }),
      })
    )

    await page.show()
    const emailInput = document.querySelector<HTMLInputElement>('#login-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#login-password')
    if (emailInput) emailInput.value = 'operator@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Check your connection, then choose Sign in again. Forge could not reach sign-in.'
    )
    expect(bodyText()).not.toContain('try signing in')
    expect(bodyText()).not.toContain('Network error')
  })

  test('turns email sign-in role failures into access guidance', async () => {
    const page = new AuthPage(
      createAuthManager({
        login: vi.fn().mockResolvedValue({
          ok: false,
          error: 'owner role required',
        }),
      })
    )

    await page.show()
    const emailInput = document.querySelector<HTMLInputElement>('#login-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#login-password')
    if (emailInput) emailInput.value = 'operator@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Ask an owner or admin to check your access. This account is not allowed to sign in here.'
    )
    expect(bodyText()).not.toContain('owner role required')
  })

  test('guides password recovery without exposing account existence', async () => {
    const page = new AuthPage(createAuthManager())

    await page.show()
    document.querySelector<HTMLAnchorElement>('#forgot-password-link')?.click()

    expect(bodyText()).toContain('Reset your password')
    expect(bodyText()).toContain('If it matches an account, we will email a reset link.')
    expect(document.querySelector('#forgot-submit')?.textContent).toContain('Email me a reset link')
  })

  test('tells new users how to finish email verification after account creation', async () => {
    const page = new AuthPage(
      createAuthManager({
        register: vi.fn().mockResolvedValue({
          ok: true,
          user: { id: 'user-1', email: 'new@example.com', username: 'new' },
        }),
      })
    )

    await page.show()
    document.querySelector<HTMLButtonElement>('[data-tab="register"]')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#register-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#register-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#register-confirm')
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain('Check your email')
    expect(bodyText()).toContain('Open that email to finish creating your account.')
    expect(bodyText()).toContain('Back to sign in')
    expect(bodyText()).not.toMatch(/[\u{1F300}-\u{1FAFF}]/u)
  })

  test('checks account creation password confirmation before calling the backend', async () => {
    const register = vi.fn().mockResolvedValue({ ok: true })
    const page = new AuthPage(createAuthManager({ register }))

    await page.show()
    document.querySelector<HTMLButtonElement>('[data-tab="register"]')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#register-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#register-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#register-confirm')
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'DifferentPassword123!'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'The two passwords do not match. Re-enter both password fields, then choose Create account and continue again.'
    )
    expect(bodyText()).not.toContain('Re-enter both password fields, then try again.')
    expect(document.querySelector<HTMLInputElement>('#register-confirm')).toBe(
      document.activeElement
    )
    expect(register).not.toHaveBeenCalled()
  })

  test('checks account creation password rules before calling the backend', async () => {
    const register = vi.fn().mockResolvedValue({ ok: true })
    const page = new AuthPage(createAuthManager({ register }))

    await page.show()
    document.querySelector<HTMLButtonElement>('[data-tab="register"]')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#register-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#register-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#register-confirm')
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'longpassword'
    if (confirmInput) confirmInput.value = 'longpassword'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'Add at least one uppercase letter to the password, then choose Create account and continue again.'
    )
    expect(document.querySelector<HTMLInputElement>('#register-password')).toBe(
      document.activeElement
    )
    expect(register).not.toHaveBeenCalled()
  })

  test('turns duplicate account registration failures into a next step', async () => {
    const page = new AuthPage(
      createAuthManager({
        register: vi.fn().mockResolvedValue({
          ok: false,
          errorCode: 'EMAIL_ALREADY_EXISTS',
          error: 'duplicate key value violates unique constraint users_email_key',
        }),
      })
    )

    await page.show()
    document.querySelector<HTMLButtonElement>('[data-tab="register"]')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#register-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#register-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#register-confirm')
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Sign in instead, or reset the password if you cannot access it. An account may already exist for this email.'
    )
    expect(bodyText()).not.toContain('duplicate key')
    expect(bodyText()).not.toContain('users_email_key')
  })

  test('turns invalid account email failures into the create button action', async () => {
    const page = new AuthPage(
      createAuthManager({
        register: vi.fn().mockResolvedValue({
          ok: false,
          errorCode: 'INVALID_EMAIL',
          error: 'invalid email address',
        }),
      })
    )

    await page.show()
    document.querySelector<HTMLButtonElement>('[data-tab="register"]')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#register-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#register-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#register-confirm')
    if (emailInput) emailInput.value = 'new'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Enter a valid email address, then choose Create account and continue again.'
    )
    expect(bodyText()).not.toContain('try creating')
    expect(bodyText()).not.toContain('invalid email address')
  })

  test('shows reset-token users what password change will affect', async () => {
    const page = new AuthPage(createAuthManager(), 'login', 'reset-token')

    await page.show()

    expect(bodyText()).toContain('Choose a new password')
    expect(bodyText()).toContain('This only changes your Wisdoverse Forge account password.')
    expect(document.querySelector('#reset-submit')?.textContent).toContain('Save new password')
  })

  test('guides reset-token users when confirmation does not match', async () => {
    const page = new AuthPage(createAuthManager(), 'login', 'reset-token')

    await page.show()
    const passwordInput = document.querySelector<HTMLInputElement>('#reset-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#reset-confirm')
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'DifferentPassword123!'
    document
      .querySelector<HTMLFormElement>('#reset-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'The two passwords do not match. Re-enter both fields, then choose Save new password again.'
    )
    expect(bodyText()).not.toContain('Re-enter both fields, then try again.')
  })

  test('guides reset-token users when the new password is too short', async () => {
    const page = new AuthPage(createAuthManager(), 'login', 'reset-token')

    await page.show()
    const passwordInput = document.querySelector<HTMLInputElement>('#reset-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#reset-confirm')
    if (passwordInput) passwordInput.value = 'short'
    if (confirmInput) confirmInput.value = 'short'
    document
      .querySelector<HTMLFormElement>('#reset-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'Use at least 12 characters for the new password. Add a few more characters, then choose Save new password again.'
    )
  })

  test('checks reset-token password rules before calling the backend', async () => {
    const resetPassword = vi.fn().mockResolvedValue(undefined)
    const page = new AuthPage(createAuthManager({ resetPassword }), 'login', 'reset-token')

    await page.show()
    const passwordInput = document.querySelector<HTMLInputElement>('#reset-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#reset-confirm')
    if (passwordInput) passwordInput.value = 'longpassword'
    if (confirmInput) confirmInput.value = 'longpassword'
    document
      .querySelector<HTMLFormElement>('#reset-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))

    expect(bodyText()).toContain(
      'Add at least one uppercase letter to the password, then choose Save new password again.'
    )
    expect(resetPassword).not.toHaveBeenCalled()
  })

  test('keeps password reset email failures beginner-safe', async () => {
    const page = new AuthPage(
      createAuthManager({
        forgotPassword: vi.fn().mockRejectedValue(new Error('SMTP tenant missing')),
      })
    )

    await page.show()
    document.querySelector<HTMLAnchorElement>('#forgot-password-link')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#forgot-email')
    if (emailInput) emailInput.value = 'operator@example.com'
    document
      .querySelector<HTMLFormElement>('#forgot-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Check the email address, wait a moment, then request the reset email again.'
    )
    expect(bodyText()).not.toContain('SMTP tenant missing')
  })

  test('explains password reset email connection failures without service jargon', async () => {
    const page = new AuthPage(
      createAuthManager({
        forgotPassword: vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
      })
    )

    await page.show()
    document.querySelector<HTMLAnchorElement>('#forgot-password-link')?.click()
    const emailInput = document.querySelector<HTMLInputElement>('#forgot-email')
    if (emailInput) emailInput.value = 'operator@example.com'
    document
      .querySelector<HTMLFormElement>('#forgot-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Check your connection, then request the reset email again. Forge could not reach email delivery.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
    expect(bodyText()).not.toMatch(/[\u{1F300}-\u{1FAFF}]/u)
  })

  test('explains expired reset links without showing raw backend text', async () => {
    const page = new AuthPage(
      createAuthManager({
        resetPassword: vi.fn().mockRejectedValue(new Error('invalid or expired token')),
      }),
      'login',
      'reset-token'
    )

    await page.show()
    const passwordInput = document.querySelector<HTMLInputElement>('#reset-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#reset-confirm')
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#reset-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Request a new reset email, then open the newest link. This reset link may have expired.'
    )
    expect(bodyText()).not.toContain('invalid or expired token')
  })

  test('explains password update connection failures without raw network text', async () => {
    const page = new AuthPage(
      createAuthManager({
        resetPassword: vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
      }),
      'login',
      'reset-token'
    )

    await page.show()
    const passwordInput = document.querySelector<HTMLInputElement>('#reset-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#reset-confirm')
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#reset-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Check your connection, then save the new password again. Forge could not reach password reset.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
  })

  test('explains password update failures with the save button action', async () => {
    const page = new AuthPage(
      createAuthManager({
        resetPassword: vi.fn().mockRejectedValue(new Error('password policy rejected')),
      }),
      'login',
      'reset-token'
    )

    await page.show()
    const passwordInput = document.querySelector<HTMLInputElement>('#reset-password')
    const confirmInput = document.querySelector<HTMLInputElement>('#reset-confirm')
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    if (confirmInput) confirmInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#reset-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Review the password checklist, enter a password that passes every item, then choose Save new password again. Password could not be updated.'
    )
    expect(bodyText()).not.toContain('password policy rejected')
    expect(bodyText()).not.toContain('Check the password rules, then try again')
  })

  test('shows a visible recovery step when verification resend fails', async () => {
    const page = new AuthPage(
      createAuthManager({
        login: vi.fn().mockResolvedValue({
          ok: false,
          errorCode: 'EMAIL_NOT_VERIFIED',
        }),
        resendVerification: vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
      })
    )

    await page.show()
    const emailInput = document.querySelector<HTMLInputElement>('#login-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#login-password')
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()
    document.querySelector<HTMLButtonElement>('#verify-resend-btn')?.click()
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'Check your connection, then send the verification email again. Forge could not reach email delivery.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
  })

  test('names verification email resend progress after a blocked sign-in', async () => {
    const request = deferred<void>()
    const resendVerification = vi.fn().mockReturnValueOnce(request.promise)
    const page = new AuthPage(
      createAuthManager({
        login: vi.fn().mockResolvedValue({
          ok: false,
          errorCode: 'EMAIL_NOT_VERIFIED',
        }),
        resendVerification,
      })
    )

    await page.show()
    const emailInput = document.querySelector<HTMLInputElement>('#login-email')
    const passwordInput = document.querySelector<HTMLInputElement>('#login-password')
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#login-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    document.querySelector<HTMLButtonElement>('#verify-resend-btn')?.click()

    expect(bodyText()).toContain('Sending verification email...')
    expect(bodyText()).not.toContain('Sending...')
    expect(resendVerification).toHaveBeenCalledWith('new@example.com')

    request.resolve()
    await flushAsyncWork()
  })
})
