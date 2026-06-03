import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

type MobileViewport = { width: number; height: number }

const IPHONE_12: MobileViewport = { width: 390, height: 844 }
const IPHONE_14_PRO_MAX: MobileViewport = { width: 430, height: 932 }

function encodeBase64Url(value: unknown): string {
  return Buffer.from(JSON.stringify(value), 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
}

async function injectMobileAuth(page: Page): Promise<void> {
  const exp = Math.floor(Date.now() / 1000) + 3600
  const token = `${encodeBase64Url({ alg: 'none', typ: 'JWT' })}.${encodeBase64Url({
    sub: 'user-e2e',
    exp,
    orgId: 'org-1',
    role: 'admin',
  })}.signature`

  await page.addInitScript(
    ({ accessToken }) => {
      localStorage.setItem('af:auth:access', accessToken)
      localStorage.setItem(
        'af:auth:user',
        JSON.stringify({
          id: 'user-e2e',
          email: 'owner@example.test',
          username: 'owner',
          orgId: 'org-1',
          role: 'admin',
        })
      )
      localStorage.setItem('af:onboarding:completed', 'true')
      localStorage.setItem('af:nav:orgId', 'org-1')
      localStorage.setItem('af:nav:projectId', 'proj-1')
      localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
    },
    { accessToken: token }
  )
}

async function installNoopWebSocket(page: Page): Promise<void> {
  await page.addInitScript(() => {
    class NoopWebSocket {
      static CONNECTING = 0
      static OPEN = 1
      static CLOSING = 2
      static CLOSED = 3

      readyState = NoopWebSocket.OPEN
      onopen: ((event: Event) => void) | null = null
      onclose: ((event: Event) => void) | null = null
      onerror: ((event: Event) => void) | null = null
      onmessage: ((event: MessageEvent) => void) | null = null

      constructor() {
        window.setTimeout(() => this.onopen?.(new Event('open')), 0)
      }

      send(): void {}

      close(): void {
        this.readyState = NoopWebSocket.CLOSED
        this.onclose?.(new Event('close'))
      }
    }

    window.WebSocket = NoopWebSocket as unknown as typeof WebSocket
  })
}

async function installMobileLoginMocks(page: Page): Promise<void> {
  await page.context().route('**/api/v1/auth/providers', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, providers: [] }),
    })
  )
  await page.context().route('**/api/v1/auth/refresh', (route) =>
    route.fulfill({
      status: 401,
      contentType: 'application/json',
      body: JSON.stringify({ ok: false, message: 'No refresh token' }),
    })
  )
  await page.context().route('**/api/v1/auth/login', async (route) => {
    const exp = Math.floor(Date.now() / 1000) + 3600
    const token = `${encodeBase64Url({ alg: 'none', typ: 'JWT' })}.${encodeBase64Url({
      sub: 'user-e2e',
      exp,
      orgId: 'org-1',
      role: 'admin',
    })}.signature`

    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        user: {
          id: 'user-e2e',
          email: 'owner@example.test',
          username: 'owner',
          orgId: 'org-1',
          role: 'admin',
        },
        tokens: { accessToken: token, expiresIn: 3600 },
      }),
    })
  })
}

async function installAgentMocks(page: Page): Promise<void> {
  await page.context().route('**/api/v1/agents', (route) => {
    if (route.request().method() !== 'GET') return route.continue()

    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        agents: [
          {
            id: 'agent-container-cli',
            name: 'Codex Container',
            runtimeId: 'af-codex-container',
            containerId: 'container-1234567890ab',
            status: 'idle',
            createdAt: Date.now() - 86_400_000,
            lastActivity: Date.now() - 60_000,
            cwd: '/workspace/agentforge',
            cliTool: 'codex',
            provider: null,
            model: null,
          },
        ],
      }),
    })
  })
}

async function openMobile(
  page: Page,
  baseURL: string,
  path = '/tasks',
  viewport: MobileViewport = IPHONE_12
): Promise<void> {
  await page.setViewportSize(viewport)
  await installNoopWebSocket(page)
  await injectMobileAuth(page)
  await page.goto(`${baseURL}${path}`)
  try {
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page.locator('[data-testid="top-bar"]').waitFor({ state: 'visible', timeout: 15_000 })
  } catch (err) {
    console.warn(`[mobile] app shell stalled on first nav to ${path}; reloading once. ${err}`)
    await page.reload()
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page.locator('[data-testid="top-bar"]').waitFor({ state: 'visible', timeout: 15_000 })
  }
}

async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  const noOverflow = await page.evaluate(() => {
    const root = document.documentElement
    return root.scrollWidth <= root.clientWidth + 2
  })
  expect(noOverflow).toBe(true)
}

test.describe('Mobile WebUI', () => {
  test('mobile login reaches the authenticated shell without horizontal overflow', async ({
    page,
    baseURL,
  }) => {
    await page.setViewportSize(IPHONE_12)
    await installNoopWebSocket(page)
    await installMobileLoginMocks(page)

    await page.goto(`${baseURL!}/login`)
    await page.locator('#auth-page').waitFor({ state: 'visible', timeout: 15_000 })
    await page.locator('#login-email').fill('owner@example.test')
    await page.locator('#login-password').fill('correct-horse-battery-staple')
    await page.locator('#login-submit').click()

    await page.waitForURL((url) => !url.pathname.endsWith('/login'), { timeout: 30_000 })
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await expectNoHorizontalOverflow(page)
  })

  test('mobile navigation reaches core WebUI sections without horizontal overflow', async ({
    page,
    baseURL,
  }) => {
    await openMobile(page, baseURL!)

    await page.getByRole('button', { name: 'Open navigation' }).click()
    await page.locator('[data-testid="sidebar"]').waitFor({ state: 'visible', timeout: 5000 })
    await page.locator('[data-testid="sidebar-nav-inbox"]').click()
    await page.waitForURL('**/inbox')
    await expect(page.getByRole('heading', { name: 'Inbox' })).toBeVisible()
    await expectNoHorizontalOverflow(page)

    await page.getByRole('button', { name: 'Open navigation' }).click()
    await page.locator('[data-testid="sidebar-nav-agents"]').click()
    await page.waitForURL('**/agents')
    await expect(page.getByRole('heading', { name: 'Agents' })).toBeVisible()
    await expectNoHorizontalOverflow(page)
  })

  test('mobile task cards open task detail as an overlay', async ({ page, baseURL }, testInfo) => {
    await openMobile(page, baseURL!, '/tasks')
    await page.locator('[data-testid="column-count-working"]').waitFor({
      state: 'attached',
      timeout: 10_000,
    })

    await page.locator('[data-testid="task-card-t-003"]').click()

    const detail = page.locator('[data-testid="right-panel"]')
    await expect(detail).toBeVisible({ timeout: 5000 })
    await expect(detail).toContainText('Write unit tests for auth module')
    await expect(detail).toContainText('Work')
    await expectNoHorizontalOverflow(page)
    await page.screenshot({
      path: testInfo.outputPath('mobile-task-detail-390x844.png'),
      fullPage: true,
    })

    await page.locator('[data-testid="detail-close"]').click()
    await expect(detail).toBeHidden({ timeout: 3000 })
  })

  test('mobile agent list opens agent detail at large-phone viewport', async ({
    page,
    baseURL,
  }, testInfo) => {
    await installAgentMocks(page)
    await openMobile(page, baseURL!, '/agents', IPHONE_14_PRO_MAX)

    await page.locator('[data-testid="agent-card-agent-container-cli"]').click()

    await expect(page.locator('[data-testid="agent-back"]')).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Codex Container' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Console' })).toBeVisible()
    await expect(page.getByText('Details')).toBeVisible()
    await expectNoHorizontalOverflow(page)
    await page.screenshot({
      path: testInfo.outputPath('mobile-agent-detail-430x932.png'),
      fullPage: true,
    })
  })

  test('mobile settings keeps credential/account sections reachable', async ({ page, baseURL }) => {
    await openMobile(page, baseURL!, '/settings', IPHONE_14_PRO_MAX)

    await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible()
    await expect(page.locator('[data-testid="settings-mobile-nav"]')).toBeVisible()

    const picker = page.locator('#settings-section-picker')
    await picker.selectOption('account')
    await expect(page.getByRole('heading', { name: 'Account' })).toBeVisible()
    await expect(page.getByText('Profile')).toBeVisible()
    await expectNoHorizontalOverflow(page)

    await picker.selectOption('keys')
    await expect(picker).toHaveValue('keys')
    await expectNoHorizontalOverflow(page)
  })
})
