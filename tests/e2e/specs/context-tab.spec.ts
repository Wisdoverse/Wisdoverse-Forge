import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

async function setupAndNavigate(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
  await page.goto('/tasks', { waitUntil: 'domcontentloaded' })
  await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30000 })
  await page.getByTestId('main-content').waitFor({ state: 'attached', timeout: 15000 })
}

async function openContextDocument(page: Page): Promise<void> {
  const card = page.getByTestId('task-card-t-003')
  await card.waitFor({ state: 'visible', timeout: 30000 })
  await card.dispatchEvent('click')
  await page.waitForURL('**/tasks/t-003')
  await page
    .getByRole('heading', { level: 1, name: 'Write unit tests for auth module' })
    .waitFor({ state: 'visible', timeout: 30000 })
}

test.describe('Task document context rail', () => {
  test('shows notes, instructions, evidence, provenance, and feedback controls', async ({
    page,
  }) => {
    await setupAndNavigate(page)
    await openContextDocument(page)

    const contextToggle = page.getByRole('button', { name: 'Context', exact: true })
    await expect(contextToggle).toBeVisible()
    await contextToggle.click()
    const contextSection = contextToggle.locator('..')

    await expect(contextSection.getByText('Saved notes used', { exact: true })).toBeVisible()
    await expect(
      contextSection.getByRole('heading', { name: 'Prod-ext validation memory' })
    ).toBeVisible()
    await expect(contextSection.getByText('Guidance used', { exact: true })).toBeVisible()
    await expect(contextSection.getByText('Review checklist')).toBeVisible()
    await expect(contextSection.getByTestId('context-evidence')).toBeVisible()
    await expect(contextSection.getByTestId('context-provenance')).toBeVisible()

    await contextSection.getByRole('button', { name: 'Useful' }).first().click()
    await expect(contextSection.getByRole('button', { name: 'Useful' }).first()).toBeVisible()
  })

  test('keeps the document reachable at a 375px viewport', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await setupAndNavigate(page)
    await openContextDocument(page)

    await expect(page.getByTestId('task-next-action')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Context', exact: true })).toBeHidden()
  })
})
