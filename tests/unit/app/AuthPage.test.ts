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

    expect(bodyText()).toContain('Team workspace access')
    expect(bodyText()).toContain('Sign in to manage agents, tasks, evidence, and team settings')
    expect(bodyText()).toContain('New here? Create an account first.')
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
    expect(document.querySelector<HTMLFormElement>('#register-form')?.style.display).toBe('')
    expect(document.querySelector('#register-form')?.textContent).toContain(
      'Create your first workspace account.'
    )
    expect(document.querySelector('#register-submit')?.textContent).toContain(
      'Create account and continue'
    )
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
      'This sign-in link expired or could not be verified. Start sign-in again from this page.'
    )
    expect(bodyText()).not.toContain('invalid_grant')
    expect(bodyText()).not.toContain('oauth token expired')
    expect(window.location.search).toBe('')
  })

  test('shows a recovery step when provider sign-in callback fails', async () => {
    window.history.replaceState({}, '', '/login?auth_code=callback-code')
    const exchangeAuthCode = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'))
    const page = new AuthPage(createAuthManager({ exchangeAuthCode }))

    await page.show()

    expect(exchangeAuthCode).toHaveBeenCalledWith('callback-code')
    expect(bodyText()).toContain(
      'Sign-in could not finish. Forge could not connect while signing you in. Check your connection, then try again.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
    expect(window.location.search).toBe('')
  })

  test('explains sign-in setup failures without provider wording', async () => {
    window.history.replaceState({}, '', '/login?auth_error=provider_not_configured')
    const page = new AuthPage(createAuthManager())

    await page.show()

    expect(bodyText()).toContain(
      'This sign-in option is not ready. Ask an owner or admin to check sign-in setup.'
    )
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
      'We could not sign you in right now. Try again in a minute. If it still fails, ask an owner or admin to check sign-in setup.'
    )
    expect(bodyText()).not.toContain('database unavailable')
    expect(bodyText()).not.toContain('HTTP 500')
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
      'Sign-in could not finish. Forge could not connect while signing you in. Check your connection, then try again.'
    )
    expect(bodyText()).not.toContain('Network error')
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
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain('Check your email')
    expect(bodyText()).toContain('Open that email to finish creating your account.')
    expect(bodyText()).toContain('Back to sign in')
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
    if (emailInput) emailInput.value = 'new@example.com'
    if (passwordInput) passwordInput.value = 'LongPassword123!'
    document
      .querySelector<HTMLFormElement>('#register-form')
      ?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await flushAsyncWork()

    expect(bodyText()).toContain(
      'An account may already exist for this email. Sign in instead, or reset the password if you cannot access it.'
    )
    expect(bodyText()).not.toContain('duplicate key')
    expect(bodyText()).not.toContain('users_email_key')
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
      'The two passwords do not match. Re-enter both fields, then try again.'
    )
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
      'Use at least 12 characters for the new password. Add a few more characters, then try again.'
    )
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
      'Reset email could not be requested. Check the email address, wait a moment, then try again.'
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
      'Reset email could not be requested. Forge could not connect while sending the reset email. Check your connection, then try again.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
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
      'This reset link may have expired. Request a new reset email, then open the newest link.'
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
      'Password could not be updated. Forge could not connect while saving your new password. Check your connection, then try again.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
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
      'Verification email could not be sent. Forge could not connect while sending it. Check your connection, then try again.'
    )
    expect(bodyText()).not.toContain('Failed to fetch')
    expect(bodyText()).not.toContain('could not reach the service')
  })
})
