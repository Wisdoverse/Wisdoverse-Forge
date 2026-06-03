/**
 * Playwright global setup — real login against live backend.
 *
 * Runs ONCE before the suite, produces `.auth/user.json` (storageState).
 * Every test inherits auth via `use: { storageState }`.
 *
 * Why a real login instead of a stitched-together mock JWT:
 *   1. Auth migrations (e.g. refresh-token → httpOnly cookie in 01e8a2f4)
 *      stop breaking the E2E suite silently — a schema change that would
 *      have cost us 18 days of undetected drift is now surfaced at setup.
 *   2. Tokens carry real `exp` / signatures, so `isTokenExpired` and any
 *      future backend claim validation behave the same as in prod.
 *   3. Refresh flows (cookie round-trip) exercise real middleware.
 *
 * The account is seeded on first run (idempotent) so this works against
 * a fresh local stack as well as staging/production-canary targets where
 * the account already exists.
 */

import { chromium, request, type FullConfig } from '@playwright/test'
import { mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const AUTH_DIR = path.resolve(here, '.auth')
const STORAGE_STATE_PATH = path.join(AUTH_DIR, 'user.json')
const CHROMIUM_EXECUTABLE_PATH =
  process.env.PLAYWRIGHT_CHROMIUM_PATH ?? '/opt/pw-browsers/chromium-1208/chrome-linux64/chrome'
const HOST_RESOLVER_RULES = process.env.E2E_HOST_RESOLVER_RULES ?? ''
const CHROMIUM_ARGS = [
  '--use-gl=swiftshader',
  '--enable-webgl',
  '--no-sandbox',
  ...(HOST_RESOLVER_RULES ? [`--host-resolver-rules=${HOST_RESOLVER_RULES}`] : []),
]

const STABLE_E2E_EMAIL = 'dev@example.com'
const E2E_EMAIL = process.env.E2E_EMAIL ?? STABLE_E2E_EMAIL
const DEFAULT_LOCAL_PASSWORD = 'DevPass123!'

function isLocalTarget(baseURL: string): boolean {
  try {
    const host = new URL(baseURL).hostname
    return host === 'localhost' || host === '127.0.0.1' || host === '::1'
  } catch {
    return false
  }
}

async function globalSetup(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0]?.use?.baseURL ?? process.env.BASE_URL
  if (!baseURL) {
    throw new Error('E2E baseURL missing: set baseURL in playwright.config.ts or BASE_URL env var')
  }

  // The local seed password is only acceptable for disposable local accounts.
  // The shared dev@example.com account may already exist with a real password, so
  // require the caller to provide it explicitly instead of guessing.
  const password = process.env.E2E_PASSWORD
  const usesStableAccount = E2E_EMAIL.toLowerCase() === STABLE_E2E_EMAIL
  if (!password && (usesStableAccount || !isLocalTarget(baseURL))) {
    throw new Error(`E2E setup: E2E_PASSWORD env var is required for ${E2E_EMAIL} on ${baseURL}`)
  }
  const E2E_PASSWORD = password ?? DEFAULT_LOCAL_PASSWORD

  await mkdir(AUTH_DIR, { recursive: true })

  // For staging behind custom DNS, route the Node HTTP call through the local API port.
  const apiBaseURL = process.env.E2E_API_BASE_URL ?? baseURL
  const api = await request.newContext({ baseURL: apiBaseURL })
  const registerResp = await api.post('/api/v1/auth/register', {
    data: { email: E2E_EMAIL, password: E2E_PASSWORD, username: 'dev' },
    failOnStatusCode: false,
  })
  if (![201, 409].includes(registerResp.status())) {
    // Any other non-success means the backend is misconfigured — fail loud.
    const body = await registerResp.text()
    await api.dispose()
    throw new Error(
      `E2E setup: register endpoint returned ${registerResp.status()} — ${body.slice(0, 200)}`
    )
  }
  await api.dispose()

  // Log in through a real browser context so cookies + localStorage end up
  // where the app actually looks for them.
  const browser = await chromium.launch({
    executablePath: CHROMIUM_EXECUTABLE_PATH,
    args: CHROMIUM_ARGS,
  })
  try {
    const context = await browser.newContext({ baseURL })
    const page = await context.newPage()

    // Navigate first so we're on the correct origin before touching localStorage.
    await page.goto('/login', { waitUntil: 'domcontentloaded' })

    const loginResult = await page.evaluate(
      async ({ email, password }) => {
        const resp = await fetch('/api/v1/auth/login', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          credentials: 'include',
          body: JSON.stringify({ email, password }),
        })
        if (!resp.ok) {
          return { ok: false, status: resp.status, body: await resp.text() }
        }
        const json = (await resp.json()) as {
          ok: boolean
          user: { id: string; email: string; username?: string; role: string; orgId: string }
          tokens: { accessToken: string; expiresIn: number }
        }
        // AuthManager persists only these two keys to localStorage today —
        // refresh token lives in an httpOnly cookie (set by the response above).
        localStorage.setItem('af:auth:access', json.tokens.accessToken)
        localStorage.setItem(
          'af:auth:user',
          JSON.stringify({
            id: json.user.id,
            email: json.user.email,
            name: json.user.username ?? json.user.email,
            role: json.user.role,
          })
        )
        localStorage.setItem('af:onboarding:completed', 'true')
        return { ok: true, status: resp.status, body: '' }
      },
      { email: E2E_EMAIL, password: E2E_PASSWORD }
    )

    if (!loginResult.ok) {
      throw new Error(
        `E2E setup: login returned ${loginResult.status} — ${loginResult.body.slice(0, 200)}`
      )
    }

    const state = await context.storageState()
    await writeFile(STORAGE_STATE_PATH, JSON.stringify(state, null, 2))
  } finally {
    await browser.close()
  }

  // Expose to later use() blocks that read process.env.
  process.env.E2E_STORAGE_STATE = STORAGE_STATE_PATH
}

export default globalSetup
export { STORAGE_STATE_PATH }
