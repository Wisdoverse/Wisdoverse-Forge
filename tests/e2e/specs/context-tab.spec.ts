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
  await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30000 })
  await page.locator('[data-testid="main-content"]').waitFor({ state: 'attached', timeout: 15000 })

  const expand = page.getByRole('button', { name: 'Show activity panel' })
  if (await expand.isVisible().catch(() => false)) {
    await expand.click()
  }
}

test.describe('Task detail Context tab', () => {
  test('shows applied memories, skills, evidence, provenance, and feedback controls', async ({
    page,
    baseURL,
  }) => {
    await setupAndNavigate(page, baseURL!)
    await page.locator('[data-testid="task-card-t-003"]').dispatchEvent('click')

    const rightPanel = page.locator('[data-testid="right-panel"]')
    await expect(rightPanel.getByRole('button', { name: 'Context', exact: true })).toBeVisible({ timeout: 5000 })
    await rightPanel.getByRole('button', { name: 'Context', exact: true }).click()

    await expect(rightPanel.getByText('Applied memories')).toBeVisible()
    await expect(
      rightPanel.getByRole('heading', { name: 'Prod-ext validation memory' })
    ).toBeVisible()
    await expect(rightPanel.getByText('Applied skills')).toBeVisible()
    await expect(rightPanel.getByText('Review checklist')).toBeVisible()
    await expect(rightPanel.getByTestId('context-evidence')).toBeVisible()
    await expect(rightPanel.getByTestId('context-provenance')).toBeVisible()

    await rightPanel.getByRole('button', { name: 'Useful' }).first().click()
    await expect(rightPanel.getByRole('button', { name: 'Useful' }).first()).toBeVisible()
  })

  test('is reachable on a 375px viewport', async ({ page, baseURL }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await setupAndNavigate(page, baseURL!)
    await page.locator('[data-testid="task-card-t-003"]').dispatchEvent('click')

    const panel = page.locator('[data-testid="right-panel"]')
    await expect(panel.getByRole('button', { name: 'Context', exact: true })).toBeVisible({ timeout: 5000 })
    await panel.getByRole('button', { name: 'Context', exact: true }).click()
    await expect(panel.getByRole('heading', { name: 'Prod-ext validation memory' })).toBeVisible()
  })
})
