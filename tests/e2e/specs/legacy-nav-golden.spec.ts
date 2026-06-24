/**
 * Legacy Nav Golden Snapshot — Phase 1 of Issue #15
 *
 * Captures the rendered sidebar org/team/project labels under the
 * deterministic mock-API regime from react-app-smoke.spec.ts.
 *
 * Why: any later phase that changes the legacy nav contract or the
 * canonical replacement must keep producing the same labels in the
 * sidebar UI. Diff in this snapshot = visible regression for the
 * legacy frontend caller.
 */
import { test, expect, type Page, type Route } from '@playwright/test'

const MOCK_ORG = { id: 'org-1', name: 'Test Org', slug: 'test-org', plan: 'pro', role: 'admin' }
const MOCK_TEAM = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Engineering',
  slug: 'engineering',
  visibility: 'private',
  description: '',
}
const MOCK_TEAM_2 = {
  id: 'team-2',
  orgId: 'org-1',
  name: 'Design',
  slug: 'design',
  visibility: 'private',
  description: '',
}
const MOCK_PROJECT = {
  id: 'proj-1',
  teamId: 'team-1',
  name: 'Wisdoverse Forge',
  slug: 'agentforge',
  color: '#007AFF',
  description: 'Main project',
}
const MOCK_PROJECT_2 = {
  id: 'proj-2',
  teamId: 'team-2',
  name: 'Marketing Site',
  slug: 'marketing-site',
  color: '#FF9500',
  description: 'Marketing project',
}
const MOCK_GROUP = { id: 'grp-1', name: 'Default', projectId: 'proj-1' }

async function injectAuth(page: Page, baseURL: string): Promise<void> {
  // Auth itself is provided by Playwright storageState (see global-setup.ts);
  // this helper only primes nav preferences so the sidebar tree matches the
  // deterministic snapshot.
  await page.goto(baseURL.replace(/\/$/, ''), { waitUntil: 'commit', timeout: 60000 })
  await page.evaluate(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', JSON.stringify(['team-1', 'team-2']))
  })
}

async function setupNavMocks(page: Page): Promise<void> {
  await page.route('**/api/v1/orgs', (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, orgs: [MOCK_ORG] }),
    })
  )
  await page.route('**/api/v1/orgs/*/teams', (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, teams: [MOCK_TEAM, MOCK_TEAM_2] }),
    })
  )
  await page.route('**/api/v1/teams/*/projects', (route: Route) => {
    const url = route.request().url()
    const projects = url.includes('team-2') ? [MOCK_PROJECT_2] : [MOCK_PROJECT]
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, projects }),
    })
  })
  await page.route('**/api/v1/groups?*', (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, data: [MOCK_GROUP], groups: [MOCK_GROUP] }),
    })
  )
}

async function waitForSidebar(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded')
  await page.waitForFunction(
    () => {
      const root = document.getElementById('root')
      return root && root.children.length > 0
    },
    { timeout: 30000 }
  )
  await page.locator('[data-testid="sidebar"]').waitFor({ state: 'attached', timeout: 15000 })
}

test.describe.serial('Legacy nav golden snapshot', () => {
  test.beforeEach(async ({ page, baseURL }) => {
    await injectAuth(page, baseURL!)
    await setupNavMocks(page)
    await page.goto(baseURL!)
    await waitForSidebar(page)
  })

  test('sidebar renders org name', async ({ page }) => {
    const sidebar = page.locator('[data-testid="sidebar"]')
    await expect(sidebar).toContainText('Test Org', { timeout: 10000 })
  })

  test('sidebar team list snapshot is stable', async ({ page }) => {
    const sidebar = page.locator('[data-testid="sidebar"]')
    // Wait for both team rows to be present
    await sidebar
      .locator('[data-testid="team-team-1"]')
      .waitFor({ state: 'attached', timeout: 10000 })
    await sidebar
      .locator('[data-testid="team-team-2"]')
      .waitFor({ state: 'attached', timeout: 10000 })

    const teams = (await sidebar.locator('[data-testid^="team-team-"]').allTextContents())
      .map((t) => t.trim())
      .filter(Boolean)
      .sort()

    expect(teams.join('\n') + '\n').toMatchSnapshot('nav-team-list.txt')
  })

  test('sidebar project list snapshot is stable', async ({ page }) => {
    const sidebar = page.locator('[data-testid="sidebar"]')
    await sidebar
      .locator('[data-testid="project-proj-1"]')
      .waitFor({ state: 'attached', timeout: 10000 })
    await sidebar
      .locator('[data-testid="project-proj-2"]')
      .waitFor({ state: 'attached', timeout: 10000 })

    const projects = (await sidebar.locator('[data-testid^="project-proj-"]').allTextContents())
      .map((t) => t.trim())
      .filter(Boolean)
      .sort()

    expect(projects.join('\n') + '\n').toMatchSnapshot('nav-project-list.txt')
  })
})
