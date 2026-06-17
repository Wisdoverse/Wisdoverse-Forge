import type { Page, Route } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

async function injectStartPreferences(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
}

async function mockProviders(page: Page, providers: object[]): Promise<void> {
  await page.route('**/api/v1/llm-providers', (route: Route) => {
    if (route.request().method() === 'GET') {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, providers }),
      })
    }
    return route.continue()
  })
  await page.route('**/api/v1/llm-providers/supported', (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        providers: [
          {
            provider: 'openai',
            displayName: 'OpenAI',
            models: [{ model: 'gpt-5.5', displayName: 'GPT-5.5' }],
          },
        ],
      }),
    })
  )
}

async function waitForAppReady(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded')
  await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30000 })
  await page.locator('[data-testid="sidebar"]').waitFor({ state: 'attached', timeout: 15000 })
}

test.describe('First-use Start checklist', () => {
  test('shows the start page with checklist heading and steps', async ({ page, baseURL }) => {
    await injectStartPreferences(page)
    await mockProviders(page, [
      {
        id: 'provider-1',
        provider: 'openai',
        displayName: 'OpenAI',
        model: 'gpt-5.5',
        priority: 0,
        isEnabled: true,
        isDefault: true,
        lastTestStatus: 'passed',
      },
    ])

    await page.goto(`${baseURL}/start`)
    await waitForAppReady(page)

    const startPage = page.locator('[data-testid="page-start"]')
    await expect(startPage).toBeVisible()
    await expect(page.getByRole('heading', { name: 'Start with one safe path' })).toBeVisible()
    await expect(startPage.getByRole('heading', { name: /Workspace/i }).first()).toBeVisible()
    await expect(startPage.getByRole('heading', { name: /Agent/i }).first()).toBeVisible()
    await expect(startPage.getByRole('heading', { name: /How agents can answer/i }).first()).toBeVisible()
    await expect(startPage.getByText('Wisdoverse Forge').first()).toBeVisible()
    await expect(startPage.getByText(/\d+ of \d+/).first()).toBeVisible()
  })

  test('agent answer setup step navigates to settings', async ({ page, baseURL }) => {
    await injectStartPreferences(page)
    await mockProviders(page, [])

    await page.goto(`${baseURL}/start`)
    await waitForAppReady(page)

    const startPage = page.locator('[data-testid="page-start"]')
    await expect(startPage).toBeVisible()
    const providerStep = startPage.getByRole('heading', { name: /How agents can answer/i }).first()
    await expect(providerStep).toBeVisible()
    const stepRow = providerStep.locator('xpath=ancestor::article')
    const actionBtn = stepRow.getByRole('button').first()
    await expect(actionBtn).toBeVisible()
    await actionBtn.click()
    await page.waitForURL(/\/(settings|agents)/)
    await expect(page.getByRole('heading', { name: /Settings|Agents/i })).toBeVisible()
  })
})
