import type { Page, Route } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'
import { mockUserPreferences } from '../fixtures/mocks'

function encodeBase64Url(value: unknown): string {
  return Buffer.from(JSON.stringify(value), 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
}

async function injectStartPreferences(page: Page): Promise<void> {
  const exp = Math.floor(Date.now() / 1000) + 3600
  const token = `${encodeBase64Url({ alg: 'none', typ: 'JWT' })}.${encodeBase64Url({
    sub: 'user-e2e',
    exp,
    orgId: 'org-1',
    role: 'admin',
  })}.signature`

  await page.route('**/api/v1/auth/refresh', (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        tokens: { accessToken: token, expiresIn: 3600 },
      }),
    })
  )
  await page.addInitScript(
    ({ accessToken }) => {
      localStorage.setItem('af:auth:access', accessToken)
      localStorage.setItem(
        'af:auth:user',
        JSON.stringify({
          id: 'user-e2e',
          email: 'dev@example.com',
          username: 'dev',
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
    await mockUserPreferences(page.context(), {
      gettingStartedDismissed: false,
    })
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
    await expect(
      page.getByRole('heading', { name: 'Set up your first agent safely' })
    ).toBeVisible()
    await expect(
      startPage.getByRole('heading', { name: /Team and project/i }).first()
    ).toBeVisible()
    await expect(startPage.getByRole('heading', { name: /Agent/i }).first()).toBeVisible()
    await expect(
      startPage.getByRole('heading', { name: /Give agents a way to work/i }).first()
    ).toBeVisible()
    await expect(startPage.getByText('Wisdoverse Forge').first()).toBeVisible()
    await expect(startPage.getByText(/\d+ of \d+/).first()).toBeVisible()
  })

  test('agent answer setup step navigates to settings', async ({ page, baseURL }) => {
    await injectStartPreferences(page)
    await mockUserPreferences(page.context(), {
      gettingStartedDismissed: false,
    })
    await mockProviders(page, [])

    await page.goto(`${baseURL}/start`)
    await waitForAppReady(page)

    const startPage = page.locator('[data-testid="page-start"]')
    await expect(startPage).toBeVisible()
    const providerStep = startPage
      .getByRole('heading', { name: /Give agents a way to work/i })
      .first()
    await expect(providerStep).toBeVisible()
    const stepRow = providerStep.locator('xpath=ancestor::article')
    const actionBtn = stepRow.getByRole('button').first()
    await expect(actionBtn).toBeVisible()
    await actionBtn.click()
    await page.waitForURL(/\/(settings|agents)/)
    await expect(page.getByRole('heading', { name: /Settings|Agents/i })).toBeVisible()
  })

  test('lets users skip the setup checklist and restore it from Settings', async ({
    page,
    baseURL,
  }) => {
    await injectStartPreferences(page)
    await mockProviders(page, [])
    const preferences = await mockUserPreferences(page.context(), {
      gettingStartedDismissed: false,
    })

    await page.goto(`${baseURL}/start`)
    await waitForAppReady(page)

    await expect(page.locator('[data-testid="page-start"]')).toBeVisible()
    await expect(page.locator('[data-testid="sidebar-nav-start"]')).toBeVisible()

    await page.getByTestId('getting-started-skip').click({ timeout: 30000 })
    await page.waitForURL(/\/tasks/)
    expect(preferences.current().gettingStartedDismissed).toBe(true)
    await expect(page.locator('[data-testid="sidebar-nav-start"]')).toHaveCount(0)

    await page.goto(`${baseURL}/start`)
    await page.waitForURL(/\/tasks/)
    await expect(page.locator('[data-testid="page-start"]')).toHaveCount(0)

    await page.locator('[data-testid="sidebar-nav-settings"]').click({ timeout: 30000 })
    await page.waitForURL(/\/settings/)
    await page
      .locator('[data-testid="settings-desktop-nav"]')
      .getByRole('link', { name: /Account: Update profile, password, and show/i })
      .click({ timeout: 30000 })

    await expect(page.getByRole('heading', { name: 'Setup checklist' })).toBeVisible()
    await expect(
      page.getByText(/hidden from the left menu, so new sign-ins open Tasks by default/i)
    ).toBeVisible()

    await page.getByRole('button', { name: /Reset setup checklist/i }).click({ timeout: 30000 })
    expect(preferences.current().gettingStartedDismissed).toBe(false)
    await expect(page.getByRole('status')).toContainText('back in the left menu')
    await expect(page.locator('[data-testid="sidebar-nav-start"]')).toBeVisible()

    await page.getByRole('button', { name: /Open setup checklist/i }).click({ timeout: 30000 })
    await page.waitForURL(/\/start/)
    await expect(page.locator('[data-testid="page-start"]')).toBeVisible()
  })
})
