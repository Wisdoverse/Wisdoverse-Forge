import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

function encodeBase64Url(value: unknown): string {
  return Buffer.from(JSON.stringify(value), 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
}

async function injectAgentAuth(page: Page): Promise<void> {
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

async function openAgents(page: Page, baseURL: string): Promise<void> {
  await installNoopWebSocket(page)
  await injectAgentAuth(page)
  await page.goto(`${baseURL}/agents`)
  try {
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page.locator('[data-testid="top-bar"]').waitFor({ state: 'visible', timeout: 15_000 })
  } catch (err) {
    console.warn(`[agent-terminal] app shell stalled on first nav; reloading once. ${err}`)
    await page.reload()
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page.locator('[data-testid="top-bar"]').waitFor({ state: 'visible', timeout: 15_000 })
  }
}

test.describe('Agent detail Terminal tab', () => {
  test('Container CLI agent exposes Terminal tab and status output', async ({ page, baseURL }) => {
    await openAgents(page, baseURL!)
    await page.locator('[data-testid="agent-card-agent-container-cli"]').click()

    await expect(page.getByRole('button', { name: 'Terminal' })).toBeVisible({ timeout: 5000 })
    await page.getByRole('button', { name: 'Terminal' }).click()
    await expect(page.locator('[data-testid="agent-terminal-tab"]')).toBeVisible({
      timeout: 10_000,
    })
    await expect(page.locator('[data-testid="agent-terminal-tab"]')).toContainText(
      'Container: container-12'
    )
  })

  test('provider prompt agent does not expose Terminal tab', async ({ page, baseURL }) => {
    await openAgents(page, baseURL!)
    await page.locator('[data-testid="agent-card-agent-provider-prompt"]').click()

    await expect(page.getByRole('button', { name: 'Chat' })).toBeVisible({ timeout: 5000 })
    await expect(page.getByRole('button', { name: 'Terminal' })).toHaveCount(0)
  })
})
