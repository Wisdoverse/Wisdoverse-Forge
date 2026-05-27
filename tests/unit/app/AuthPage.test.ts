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

  test('explains provider sign-in and account creation in beginner language', async () => {
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

  test('shows reset-token users what password change will affect', async () => {
    const page = new AuthPage(createAuthManager(), 'login', 'reset-token')

    await page.show()

    expect(bodyText()).toContain('Choose a new password')
    expect(bodyText()).toContain('This only changes your Wisdoverse Forge account password.')
    expect(document.querySelector('#reset-submit')?.textContent).toContain('Save new password')
  })
})
