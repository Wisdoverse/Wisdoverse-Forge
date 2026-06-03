import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

async function setupAndNavigate(page: Page, baseURL: string): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
  await page.goto(`${baseURL}/tasks`)
  try {
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page
      .locator('[data-testid="main-content"]')
      .waitFor({ state: 'attached', timeout: 15_000 })
  } catch (err) {
    console.warn(`[context-preview] app shell stalled on first nav; reloading once. ${err}`)
    await page.reload()
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page
      .locator('[data-testid="main-content"]')
      .waitFor({ state: 'attached', timeout: 15_000 })
  }
}

test.describe('Context injection preview', () => {
  test('lets a user remove and pin context before publishing', async ({ page, baseURL }) => {
    let publishBody: Record<string, unknown> | null = null
    await page.route('**/api/v1/orchestration/tasks/*/publish-with-context', async (route) => {
      publishBody = route.request().postDataJSON() as Record<string, unknown>
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          ok: true,
          task: {
            id: 't-001',
            groupId: 'grp-1',
            state: 'working',
            method: 'agents/run',
            params: { task: 'Implement login flow', message: '' },
            assignedTo: 'agent-preview-1',
            assignedAgentName: 'Codex Preview',
            priority: 'high',
            progress: 0,
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
          },
        }),
      })
    })

    await setupAndNavigate(page, baseURL!)
    await page
      .getByTestId('task-card-t-001')
      .getByRole('button', { name: 'Publish Implement login flow' })
      .click()

    const dialog = page.getByRole('dialog', { name: /review context before publishing/i })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByText('Prod-ext validation memory')).toBeVisible()
    await expect(dialog.getByText('Pinned migration note')).toBeVisible()
    await dialog.getByRole('checkbox', { name: /Rollback memory/ }).click()
    await dialog.getByRole('button', { name: /Pinned migration note.*pinned/i }).click()
    await dialog.getByRole('button', { name: 'Publish with selected context' }).click()

    await expect(dialog).toBeHidden()
    expect(publishBody).toMatchObject({
      contextPreviewId: 'preview-e2e-1',
      previewHash: 'preview-hash-e2e',
      pinnedIds: ['memory-pinned'],
      removedIds: ['memory-rollback'],
    })
    await expect(
      page.locator('[data-testid="task-card-t-001"]').getByText('Codex Preview')
    ).toBeVisible()
  })

  test('is operable on a 375px viewport', async ({ page, baseURL }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await setupAndNavigate(page, baseURL!)
    await page
      .getByTestId('task-card-t-001')
      .getByRole('button', { name: 'Publish Implement login flow' })
      .click()

    const dialog = page.getByRole('dialog', { name: /review context before publishing/i })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByRole('button', { name: 'Publish with selected context' })).toBeVisible()
  })
})
