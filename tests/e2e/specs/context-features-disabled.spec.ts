import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'
import { mockContextFeatures } from '../fixtures/mocks'

const DISABLED_CONTEXT_FEATURES = {
  governance: false,
  preview: false,
  injection: false,
  analytics: false,
}

async function seedAuth(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
}

test.describe('Context feature gates', () => {
  test('hide governed context surfaces when all flags are disabled', async ({
    page,
    context,
    baseURL,
  }) => {
    await mockContextFeatures(context, DISABLED_CONTEXT_FEATURES)
    await seedAuth(page)

    await page.goto(`${baseURL}/context`, { waitUntil: 'domcontentloaded' })
    await expect(page).toHaveURL(/\/tasks/)
    await page.locator('[data-testid="main-content"]').waitFor({ state: 'attached' })

    await expect(page.getByTestId('sidebar-nav-context')).toHaveCount(0)
    await expect(
      page.getByTestId('task-card-t-001').getByRole('button', {
        name: 'Publish Implement login flow',
      })
    ).toHaveCount(0)
  })
})
