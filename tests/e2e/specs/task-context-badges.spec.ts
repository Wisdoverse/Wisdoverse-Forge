import type { BrowserContext, Page } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { test, expect } from '../fixtures/app-fixtures'

const taskWithContext = JSON.parse(
  readFileSync(new URL('../fixtures/test-data/task_with_context.json', import.meta.url), 'utf8')
)

async function installTaskFixture(context: BrowserContext) {
  await context.route('**/api/v1/orchestration/groups/*/tasks*', async (route) => {
    if (route.request().method() !== 'GET') return route.fallback()
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, tasks: [taskWithContext] }),
    })
  })
}

async function setupAndNavigate(page: Page, baseURL: string) {
  await page.addInitScript(() => {
    class MockWebSocket {
      onopen: (() => void) | null = null
      onmessage: ((event: { data: string }) => void) | null = null
      onclose: (() => void) | null = null
      readyState = 1

      constructor() {
        window.setTimeout(() => this.onopen?.(), 0)
      }

      send() {}
      close() {
        this.readyState = 3
        this.onclose?.()
      }
    }

    Object.defineProperty(window, 'WebSocket', {
      value: MockWebSocket,
      configurable: true,
      writable: true,
    })

    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })

  await page.goto(`${baseURL}/tasks`)
  await page.locator('[data-testid="task-card-task-context-fixture"]').waitFor({ state: 'visible' })
}

test.describe('Task context badges', () => {
  test('shows applied memory and skill counts from task summary', async ({
    context,
    page,
    baseURL,
  }) => {
    await installTaskFixture(context)
    await setupAndNavigate(page, baseURL!)

    const badge = page.locator('[data-testid="task-context-badge"]')
    await expect(badge).toBeVisible()
    await expect(badge).toHaveAttribute(
      'aria-label',
      '2 saved notes added, 1 saved instruction added'
    )
    await expect(badge).toContainText('2')
    await expect(badge).toContainText('1')
  })

  test('keeps context badge visible at 375px without hover', async ({ context, page, baseURL }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await installTaskFixture(context)
    await setupAndNavigate(page, baseURL!)

    const badge = page.locator('[data-testid="task-context-badge"]')
    await expect(badge).toBeVisible()
    await expect(badge).toHaveAttribute(
      'aria-label',
      '2 saved notes added, 1 saved instruction added'
    )
  })
})
