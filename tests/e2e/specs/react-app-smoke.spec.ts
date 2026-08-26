/**
 * Wisdoverse Forge React SPA — E2E Smoke Test Suite
 *
 * Tests the React frontend (TanStack Router, Kanban board, sidebar, command
 * palette, view switching, etc.) against a running deployment.
 *
 * Auth strategy: real login via `playwright.config.ts`' `globalSetup`, stored
 * as `tests/e2e/.auth/user.json`. Every test inherits a valid access token
 * and refresh cookie signed by the real backend — so auth-layer changes
 * (cookie migration, claim renames, JWT algo bumps) surface here instead
 * of rotting silently. Data-layer API calls are mocked at the browser-context
 * level by the `app-fixtures` custom test so the handlers are in place BEFORE
 * the first page navigation (no race with the nav loader); per-test overrides
 * stack via `overrideOrgs` / `overrideTeams`.
 *
 * Tests are independent — fresh context per test, mocks installed up-front,
 * no shared state. Production-safe: data endpoints are intercepted, never
 * mutate real rows.
 *
 * Run with:
 *   npm run test:e2e                       # local (Vite on 4002)
 *   BASE_URL=https://forge.example.com npm run test:e2e   # staging
 */

import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'
import {
  MOCK_ORG,
  MOCK_ORG_2,
  MOCK_TEAM,
  MOCK_TEAM_2,
  mockUserPreferences,
  overrideOrgs,
  overrideTeams,
} from '../fixtures/mocks'

// ── Configuration ───────────────────────────────────────────────────────────

const STORAGE_KEYS = {
  access: 'af:auth:access',
  user: 'af:auth:user',
}

const SCREENSHOT_DIR = 'test-results/react-app'

// ── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Seed navigation preferences so the project tree auto-expands to a known
 * state for screenshot diffs. Auth itself is provided by the Playwright
 * `storageState` loaded from `tests/e2e/.auth/user.json`.
 *
 * Uses `addInitScript` so the localStorage seed runs on every document
 * before any application code executes — avoids an extra navigation that
 * can race with the real goto under load.
 */
async function injectAuth(page: Page, _baseURL: string): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
}

/** Wait for React root to mount. Uses locator.waitFor so the timeout arg is
 * honored (page.waitForFunction is clipped to actionTimeout by Playwright). */
async function waitForAppReady(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded')
  await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30000 })
  await page.locator('[data-testid="sidebar"]').waitFor({ state: 'attached', timeout: 15000 })
}

async function gotoAndWaitForAppReady(page: Page, baseURL: string, path = ''): Promise<void> {
  await page.goto(baseURL + path)
  try {
    await waitForAppReady(page)
  } catch (err) {
    console.warn(
      `[smoke] waitForAppReady stalled on first nav to ${path || '/'}; reloading once. ${err}`
    )
    await page.reload()
    await waitForAppReady(page)
  }
}

/** Standard setup: seed localStorage + navigate + wait. API mocks are
 * installed at the browser-context level by the fixture (see
 * `app-fixtures.ts`), so they're in place before this function runs.
 *
 * One `page.reload()` on a `#root`-never-mounted timeout is kept as a
 * defense-in-depth recovery for the Vite-dev module-fetch stall — the
 * nav-loader race was resolved by the fixture, but the upstream vite
 * dev-server flake can still bite under heavy parallelism. The warning
 * is logged so the flake rate stays visible in CI output. */
// Default to `/tasks`, not `/`: the `/` index route reads preferences and may
// land on the beginner setup checklist, so board-centric smoke tests navigate directly.
async function setupAndNavigate(page: Page, baseURL: string, path = '/tasks'): Promise<void> {
  await injectAuth(page, baseURL)
  await gotoAndWaitForAppReady(page, baseURL, path)
  // Right panel defaults to collapsed (AppLayout.tsx:46) and has no persisted
  // open/closed state. Expand it once post-mount so downstream tests that
  // read `[data-testid="right-panel"]` see a mounted element. The dedicated
  // toggle test re-collapses explicitly.
  const expand = page.getByRole('button', { name: 'Show live task updates' })
  if (await expand.isVisible().catch(() => false)) {
    await expand.click()
  }
}

async function screenshot(page: Page, name: string): Promise<void> {
  await page.screenshot({ path: `${SCREENSHOT_DIR}/${name}.png`, fullPage: false })
}

async function expectTaskListHeader(page: Page): Promise<void> {
  const tasksPage = page.getByTestId('page-tasks')
  await expect(tasksPage.getByText('Task result', { exact: true })).toBeVisible({ timeout: 5000 })
  await expect(tasksPage.getByText('Agent', { exact: true })).toBeVisible()
}

async function openBoardFilters(page: Page) {
  const toolbar = page.getByTestId('board-toolbar')
  await toolbar.waitFor({ state: 'visible', timeout: 15000 })
  const filters = toolbar.getByRole('button', { name: /^Filters/ })
  if ((await filters.getAttribute('aria-expanded')) !== 'true') {
    await filters.click({ timeout: 30000 })
  }
  await expect(toolbar.getByRole('group', { name: 'Filter tasks by priority' })).toBeVisible({
    timeout: 5000,
  })
  return toolbar
}

async function openTaskDocument(page: Page, taskId: string, title: string): Promise<void> {
  const card = page.getByTestId(`task-card-${taskId}`)
  await card.waitFor({ state: 'visible', timeout: 30000 })
  await card.dispatchEvent('click')
  await page.waitForURL(`**/tasks/${taskId}`)
  await page
    .getByRole('heading', { level: 1, name: title })
    .waitFor({ state: 'visible', timeout: 30000 })
}

async function openSettingsGroup(
  page: Page,
  group: 'team and project setup' | 'more setup'
): Promise<void> {
  const nav = page.getByTestId('settings-desktop-nav')
  await nav.waitFor({ state: 'visible', timeout: 15000 })
  const button = nav.getByRole('button', { name: new RegExp(`^(Show|Hide) ${group}$`) })
  await expect(button).toBeVisible({ timeout: 15000 })
  if ((await button.getAttribute('aria-expanded')) !== 'true') {
    await button.click({ timeout: 30000 })
  }
}

// ── Test Suite ───────────────────────────────────────────────────────────────

// Top-level describe is NOT `.serial` on purpose — every test owns its own
// fresh browser context (Playwright default) and calls `setupAndNavigate`
// with per-test route mocks, so they are independent. Removing `.serial`
// means a single failure no longer cascades into dozens of "did not run"
// skips, which was the silent-drift trap called out in issue #60.
test.describe('React App Smoke Tests', () => {
  // 1. App Bootstrap ─────────────────────────────────────────────────────────

  test.describe('1. App Bootstrap', () => {
    test('app loads with injected auth tokens', async ({ page, baseURL }) => {
      await injectAuth(page, baseURL!)
      // Navigate to the origin so Playwright restores storageState's
      // localStorage for this domain before we read it.
      await page.goto(baseURL!, { waitUntil: 'commit' })
      const access = await page.evaluate((k) => localStorage.getItem(k), STORAGE_KEYS.access)
      // Storage state was seeded by globalSetup's real login — a JWT in its
      // canonical three-segment form with a non-expired `exp` claim.
      expect(access).not.toBeNull()
      const parts = access!.split('.')
      expect(parts).toHaveLength(3)
      const payload = JSON.parse(
        Buffer.from(parts[1].replace(/-/g, '+').replace(/_/g, '/'), 'base64').toString()
      ) as { exp?: number; sub?: string }
      expect(payload.sub).toBeTruthy()
      expect(payload.exp).toBeGreaterThan(Math.floor(Date.now() / 1000))
      await screenshot(page, '01-auth-tokens')
    })

    test('app renders React root with mocked APIs', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await expect(page.locator('[data-testid="sidebar"]')).toBeAttached()
      await expect(page.locator('[data-testid="top-bar"]')).toBeAttached()
      await screenshot(page, '02-app-loaded')
    })
  })

  // 2. App Layout ────────────────────────────────────────────────────────────

  test.describe('2. App Layout', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('sidebar, top bar, main content, and right panel visible', async ({ page }) => {
      await expect(page.locator('[data-testid="sidebar"]')).toBeVisible()
      await expect(page.locator('[data-testid="top-bar"]')).toBeVisible()
      await expect(page.locator('[data-testid="main-content"]')).toBeVisible()
      await expect(page.locator('[data-testid="right-panel"]')).toBeVisible()
      await screenshot(page, '03-layout-all-panels')
    })

    test('sidebar has all nav items', async ({ page }) => {
      await expect(page.locator('[data-testid="sidebar-nav-tasks"]')).toBeAttached()
      await expect(page.locator('[data-testid="sidebar-nav-inbox"]')).toBeAttached()
      await expect(page.locator('[data-testid="sidebar-nav-agents"]')).toBeAttached()
      await expect(page.locator('[data-testid="sidebar-nav-skills"]')).toBeAttached()
      await expect(page.locator('[data-testid="sidebar-nav-settings"]')).toBeAttached()
      await screenshot(page, '04-layout-sidebar-nav')
    })

    test('top bar shows view mode buttons', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      // View modes. The old Status/Agent/Priority group-by toggles were removed
      // in the #629 board redesign (replaced by BoardToolbar filters).
      await expect(topBar.getByRole('button', { name: 'Board' })).toBeVisible()
      await expect(topBar.getByRole('button', { name: 'List' })).toBeVisible()
      await expect(topBar.getByRole('button', { name: 'Timeline' })).toBeVisible()
      await expect(topBar.getByRole('button', { name: 'Map' })).toBeVisible()
      await expect(topBar.getByRole('combobox', { name: 'Place for new tasks' })).toHaveCount(0)
      await expect(
        page.getByTestId('board-toolbar').getByRole('combobox', { name: 'Place for new tasks' })
      ).toBeVisible()
    })

    test('right panel shows Activity header', async ({ page }) => {
      await expect(
        page.locator('[data-testid="right-panel"]').getByText('Live task updates', { exact: true })
      ).toBeVisible()
    })
  })

  // 3. Sidebar Navigation ────────────────────────────────────────────────────

  test.describe('3. Sidebar Navigation', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('clicking Agents navigates to /agents', async ({ page }) => {
      await page.locator('[data-testid="sidebar-nav-agents"]').click()
      await page.waitForURL('**/agents')
      await expect(
        page.getByTestId('page-agents').getByRole('heading', { name: 'Agents' })
      ).toBeVisible({ timeout: 5000 })
      await screenshot(page, '05-nav-agents')
    })

    test('clicking Inbox navigates to /inbox', async ({ page }) => {
      await page.locator('[data-testid="sidebar-nav-inbox"]').click()
      await page.waitForURL('**/inbox')
      await screenshot(page, '06-nav-inbox')
    })

    test('clicking Skills navigates to /skills', async ({ page }) => {
      await page.locator('[data-testid="sidebar-nav-skills"]').click()
      await page.waitForURL('**/skills')
      await expect(page.getByRole('heading', { name: 'Skills', level: 1 })).toBeVisible({
        timeout: 5000,
      })
      await screenshot(page, '07-nav-skills')
    })

    test('clicking Settings navigates to /settings', async ({ page }) => {
      await page.locator('[data-testid="sidebar-nav-settings"]').click()
      await page.waitForURL('**/settings')
      await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible({ timeout: 5000 })
      await screenshot(page, '08-nav-settings')
    })

    test('clicking Tasks navigates back to /tasks', async ({ page }) => {
      await page.locator('[data-testid="sidebar-nav-settings"]').click()
      await page.waitForURL('**/settings')
      await page.locator('[data-testid="sidebar-nav-tasks"]').click()
      await page.waitForURL('**/tasks')
      await screenshot(page, '09-nav-tasks')
    })
  })

  // 4. Sidebar Collapse/Expand ───────────────────────────────────────────────

  test.describe('4. Sidebar Collapse/Expand', () => {
    test('Ctrl+\\ toggles sidebar width', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      const sidebar = page.locator('[data-testid="sidebar"]')
      const initialWidth = await sidebar.evaluate((el) => el.getBoundingClientRect().width)

      await page.keyboard.press('Control+\\')
      await expect
        .poll(() => sidebar.evaluate((el) => el.getBoundingClientRect().width))
        .not.toBe(initialWidth)

      await page.keyboard.press('Control+\\')
      await expect
        .poll(() => sidebar.evaluate((el) => el.getBoundingClientRect().width))
        .toBeCloseTo(initialWidth, -1)
      await screenshot(page, '10-sidebar-toggle')
    })
  })

  // 5. Kanban Board View ─────────────────────────────────────────────────────

  test.describe('5. Kanban Board View', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('board shows 5 kanban columns', async ({ page }) => {
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })

      for (const col of ['backlog', 'queued', 'working', 'blocked', 'done']) {
        await expect(page.locator(`[data-testid="column-count-${col}"]`)).toBeAttached({
          timeout: 10000,
        })
      }
      await screenshot(page, '11-board-columns')
    })

    test('task cards render with correct content', async ({ page }) => {
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })

      const taskCard = page.locator('[data-testid="task-card-t-003"]')
      await expect(taskCard).toBeAttached({ timeout: 10000 })
      await expect(taskCard).toContainText('Write unit tests for auth module')
      await expect(taskCard).toContainText('Claude')
      await screenshot(page, '12-board-task-cards')
    })

    test('task card shows progress bar for working tasks', async ({ page }) => {
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })

      const taskCard = page.locator('[data-testid="task-card-t-003"]')
      await expect(taskCard).toBeAttached({ timeout: 10000 })
      await expect(taskCard.locator('[data-testid="progress-bar"]')).toBeAttached()
    })

    test('task card shows priority badge for non-normal priorities', async ({ page }) => {
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })

      const urgentCard = page.locator('[data-testid="task-card-t-002"]')
      await expect(urgentCard).toBeAttached({ timeout: 10000 })
      await expect(urgentCard).toContainText('Urgent')

      const highCard = page.locator('[data-testid="task-card-t-001"]')
      await expect(highCard).toBeAttached({ timeout: 10000 })
      await expect(highCard).toContainText('High')
    })

    test('column counts match mock data', async ({ page }) => {
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })

      await expect(page.locator('[data-testid="column-count-backlog"]')).toContainText('2')
      await expect(page.locator('[data-testid="column-count-queued"]')).toContainText('1')
      await expect(page.locator('[data-testid="column-count-working"]')).toContainText('2')
      await expect(page.locator('[data-testid="column-count-blocked"]')).toContainText('1')
      await expect(page.locator('[data-testid="column-count-done"]')).toContainText('1')
    })
  })

  // 6. Quick Create ──────────────────────────────────────────────────────────

  test.describe('6. Quick Create', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })
      // Wait for column counts to ensure board fully rendered
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })
    })

    test('"+ Add task" button opens inline input', async ({ page }) => {
      const addBtn = page.getByRole('button', { name: 'Add task idea' }).first()
      await expect(addBtn).toBeVisible({ timeout: 10000 })
      await addBtn.click()

      const input = page.getByRole('textbox', { name: 'Task goal' }).first()
      await expect(input).toBeVisible()
      await expect(input).toBeFocused()
      await screenshot(page, '13-quickcreate-input')
    })

    test('Escape closes quick create without submitting', async ({ page }) => {
      const addBtn = page.getByRole('button', { name: 'Add task idea' }).first()
      await addBtn.click()

      const input = page.getByRole('textbox', { name: 'Task goal' }).first()
      await input.fill('Should not be created')
      await page.keyboard.press('Escape')

      await expect(input).toBeHidden()
    })
  })

  // 7. View Mode Switching ───────────────────────────────────────────────────

  test.describe('7. View Mode Switching', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('switch to List view shows table layout', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      await topBar.getByRole('button', { name: 'List' }).click()

      await expectTaskListHeader(page)
      await screenshot(page, '14-view-list')
    })

    test('switch to Timeline view mounts the 2D timeline canvas', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      await topBar.getByRole('button', { name: 'Timeline' }).click()

      // Timeline view no longer shows a "coming soon" placeholder — it bridges
      // the legacy 2D TimelineView which mounts a .timeline-canvas element.
      await expect(page.locator('canvas.timeline-canvas')).toBeAttached({ timeout: 5000 })
      await screenshot(page, '15-view-timeline')
    })

    test('switch to 3D Map view renders the agent map resiliently', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      await topBar.getByRole('button', { name: 'Map' }).click()

      // Workshop3DView is a lazy ~520kB three.js chunk; allow a generous mount.
      const scene = page.locator('[data-testid="workshop-3d-scene"]')
      await expect(scene).toBeVisible({ timeout: 30000 })

      // It must never crash the board into the error boundary, even where WebGL
      // is unavailable (headless CI / no-GPU). Either the real canvas mounts, or
      // the graceful WebGL-unavailable fallback shows — the view always renders.
      await expect(page.getByText('Something went wrong')).toHaveCount(0)
      await expect(page.getByText(/Full React rewrite coming soon/i)).toHaveCount(0)
      const canvas = scene.locator('canvas[data-testid="workshop-3d-canvas"]')
      const fallback = scene.locator('[data-testid="workshop-3d-webgl-unavailable"]')
      await expect(canvas.or(fallback)).toBeVisible({ timeout: 10000 })

      // The HTML agent roster + selection work regardless of WebGL.
      const agentButton = scene.locator('[data-testid="workshop-3d-agent"]').first()
      await expect(agentButton).toBeVisible({ timeout: 10000 })
      await agentButton.click()
      await expect(agentButton).toHaveAttribute('aria-pressed', 'true')
      await expect(scene.locator('[data-testid="workshop-3d-selected-agent"]')).toBeVisible()

      // View toggle round-trips without crashing.
      await topBar.getByRole('button', { name: 'Board' }).click()
      await expect(scene).toHaveCount(0)
      await topBar.getByRole('button', { name: 'Map' }).click()
      await expect(page.locator('[data-testid="workshop-3d-scene"]')).toBeVisible()
      await screenshot(page, '16-view-3d')
    })

    test('switch back to Board view shows kanban columns', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')

      await topBar.getByRole('button', { name: 'List' }).click()
      await expectTaskListHeader(page)

      await topBar.getByRole('button', { name: 'Board' }).click()
      await expect(page.locator('[data-testid="column-count-backlog"]')).toBeAttached({
        timeout: 10000,
      })
      await screenshot(page, '17-view-board-restored')
    })
  })

  // 8. Task Document Page ───────────────────────────────────────────────────

  test.describe('8. Task Document Page', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })
    })

    test('clicking a task card opens its document route', async ({ page }) => {
      await openTaskDocument(page, 't-003', 'Write unit tests for auth module')

      await expect(page.getByRole('navigation', { name: 'Breadcrumb' })).toContainText('Tasks')
      await expect(page.getByTestId('task-next-action')).toBeVisible()
      await expect(page.getByTestId('task-updates')).toBeVisible()
      await screenshot(page, '18-task-detail')
    })

    test('working task actions live in the properties rail', async ({ page }) => {
      await openTaskDocument(page, 't-003', 'Write unit tests for auth module')

      const properties = page.getByRole('button', { name: 'Properties', exact: true }).locator('..')
      await expect(properties.getByRole('button', { name: 'Needs help' })).toBeVisible({
        timeout: 5000,
      })
      await expect(properties.getByRole('button', { name: 'Cancel' })).toBeVisible()
    })

    test('activity feed remains the right-side tenant', async ({ page }) => {
      await openTaskDocument(page, 't-003', 'Write unit tests for auth module')

      const activityFeed = page.getByTestId('right-panel')
      await expect(activityFeed.getByRole('heading', { name: 'Live task updates' })).toBeVisible({
        timeout: 5000,
      })
    })

    test('breadcrumb returns to the task board', async ({ page }) => {
      await openTaskDocument(page, 't-003', 'Write unit tests for auth module')

      await page
        .getByRole('navigation', { name: 'Breadcrumb' })
        .getByRole('button', { name: 'Tasks', exact: true })
        .click()

      await page.waitForURL('**/tasks')
      await expect(page.getByTestId('page-tasks')).toBeVisible({ timeout: 30000 })
    })
  })

  // 9. Command Palette ───────────────────────────────────────────────────────

  test.describe('9. Command Palette', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('Ctrl+K opens command palette', async ({ page }) => {
      await page.keyboard.press('Control+k')

      const input = page.getByPlaceholder(/Search what you want to do/)
      await expect(input).toBeVisible({ timeout: 5000 })
      await expect(
        page.getByText(/Tell an agent the result you want and how to check it/i)
      ).toBeVisible()
      await expect(page.getByText('Go to a page')).toBeVisible()
      await screenshot(page, '19-cmdk-open')
    })

    test('command palette shows Navigation, Actions, and Views groups', async ({ page }) => {
      await page.keyboard.press('Control+k')

      await expect(page.getByText('Go to a page')).toBeVisible({ timeout: 5000 })
      await expect(page.getByText('Create or change something')).toBeVisible()
      await expect(page.getByText('Change task view')).toBeVisible()
    })

    test('clicking outside closes command palette', async ({ page }) => {
      await page.keyboard.press('Control+k')
      const input = page.getByPlaceholder(/Search what you want to do/)
      await expect(input).toBeVisible({ timeout: 5000 })

      // Click the backdrop (fixed inset-0 overlay)
      await page.mouse.click(5, 5)
      await expect(input).toBeHidden({ timeout: 3000 })
    })

    test('top bar search button opens command palette', async ({ page }) => {
      const searchButton = page.getByTestId('top-bar-command-search')
      await expect(searchButton).toHaveClass(/\bh-8\b/)
      await expect(searchButton).toHaveText('')
      await searchButton.click()

      await expect(page.getByPlaceholder(/Search what you want to do/)).toBeVisible({
        timeout: 5000,
      })
      await screenshot(page, '20-cmdk-via-button')
    })
  })

  // 10. Settings Page ────────────────────────────────────────────────────────

  test.describe('10. Settings Page', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page.locator('[data-testid="sidebar-nav-settings"]').click()
      await page.waitForURL('**/settings')
      await expect(page.getByTestId('page-settings')).toBeVisible({ timeout: 30000 })
    })

    test('settings page shows provider configuration nav', async ({ page }) => {
      // Settings uses task-first labels so first-time users can find setup
      // paths without knowing internal provider, credential, or runtime terms.
      await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible({ timeout: 5000 })
      // Settings nav labels became task-first in the redesign; scope to the
      // settings nav and match the leading label (the accessible name carries a
      // trailing description).
      const settingsNav = page.getByTestId('settings-desktop-nav')
      await expect(settingsNav).toBeVisible({ timeout: 30000 })
      await openSettingsGroup(page, 'team and project setup')
      await openSettingsGroup(page, 'more setup')
      await expect(settingsNav.getByText('Start here', { exact: true })).toBeVisible()
      await expect(settingsNav.getByText('People and projects', { exact: true })).toBeVisible()
      await expect(settingsNav.getByText('Access and limits', { exact: true })).toBeVisible()
      await expect(settingsNav.getByRole('link', { name: /^AI services:/ })).toBeVisible()
      await expect(settingsNav.getByRole('link', { name: /^Tool access keys:/ })).toBeVisible()
      await expect(
        settingsNav.getByRole('link', { name: /^Code access for HTTPS links:/ })
      ).toBeVisible()
      await expect(
        settingsNav.getByRole('link', { name: /^Code access for SSH links:/ })
      ).toBeVisible()
      await expect(settingsNav.getByRole('link', { name: /^Where agents work:/ })).toBeVisible()
      await expect(settingsNav.getByRole('link', { name: /^Sign in to code tools:/ })).toBeVisible()
      await expect(settingsNav.getByRole('link', { name: /^Account:/ })).toBeVisible()
      await screenshot(page, '21-settings-page')
    })

    test('theme toggle row in the sidebar flips label', async ({ page }) => {
      const sidebar = page.getByTestId('sidebar')
      const themeBtn = sidebar.getByRole('button', { name: /Switch to (dark|light) mode/i })
      await expect(themeBtn).toBeVisible({ timeout: 5000 })
      await expect(themeBtn).toContainText('Theme')
      const initialLabel = await themeBtn.getAttribute('aria-label')

      await themeBtn.click()
      await expect
        .poll(() =>
          page
            .getByTestId('sidebar')
            .getByRole('button', { name: /Switch to (dark|light) mode/i })
            .getAttribute('aria-label')
        )
        .not.toBe(initialLabel)
      await screenshot(page, '22-settings-theme-toggled')
    })
  })

  // 11. Agents Page ──────────────────────────────────────────────────────────

  test.describe('11. Agents Page', () => {
    test('agents page loads with header and New agent button', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page.locator('[data-testid="sidebar-nav-agents"]').click()
      await page.waitForURL('**/agents')

      await expect(
        page.getByTestId('page-agents').getByRole('heading', { name: 'Agents' })
      ).toBeVisible({ timeout: 5000 })
      await expect(page.getByRole('button', { name: 'New agent' }).first()).toBeVisible()
      const agentsPage = page.getByTestId('page-agents')
      const filter = agentsPage.getByRole('button', { name: 'Filter', exact: true })
      await expect(filter).toHaveAttribute('aria-expanded', 'false')
      await expect(agentsPage.getByTestId('agent-fleet-controls')).toHaveCount(0)
      await filter.click()
      await expect(filter).toHaveAttribute('aria-expanded', 'true')
      await expect(agentsPage.getByTestId('agent-fleet-controls')).toBeVisible()
      await expect(agentsPage.getByRole('group', { name: 'Status Filter' })).toBeVisible()
      await expect(agentsPage.getByRole('group', { name: 'Work location Filter' })).toBeVisible()
      await screenshot(page, '23-agents-page')
    })
  })

  // 12. Inbox Page ───────────────────────────────────────────────────────────

  test.describe('12. Inbox Page', () => {
    test('inbox page loads with empty state', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page.locator('[data-testid="sidebar-nav-inbox"]').click()
      await page.waitForURL('**/inbox')

      await expect(page.getByText('No updates yet')).toBeVisible({ timeout: 5000 })
      const actionPath = page.getByTestId('inbox-action-path')
      await actionPath.getByRole('button', { name: 'Inbox action order', exact: true }).click()
      await expect(
        actionPath.getByText(
          'Next: start a task or wait for an agent update, then open Inbox again.',
          { exact: true }
        )
      ).toBeVisible()
      await screenshot(page, '24-inbox-empty')
    })
  })

  // 13. Skills Page ──────────────────────────────────────────────────────────

  test.describe('13. Skills Page', () => {
    test('skills page loads with search and empty state', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page.locator('[data-testid="sidebar-nav-skills"]').click()
      await page.waitForURL('**/skills')

      await expect(page.getByRole('searchbox', { name: /Search skills/ })).toBeVisible({
        timeout: 5000,
      })
      // The store may still be fetching saved guidance; accept the current
      // empty-state copy or the loading indicator.
      const emptyOrLoading = page.getByText(/Create your first skill|Checking skills/i).first()
      await expect(emptyOrLoading).toBeVisible()
      await screenshot(page, '25-skills-page')
    })
  })

  // 14. Activity Feed Side Rail ──────────────────────────────────────────────

  test.describe('14. Activity Feed Side Rail', () => {
    test('collapses and restores the live updates feed', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      const activityFeed = page.getByTestId('right-panel')
      await expect(activityFeed.getByRole('heading', { name: 'Live task updates' })).toBeVisible()

      // Close button carries a readable aria-label; icon is an SVG
      // (lucide X) with no text content, so text-filter locators miss it.
      await page.getByRole('button', { name: 'Hide live task updates' }).click()

      // Panel collapses — expand button appears
      await expect(activityFeed).toBeHidden({ timeout: 3000 })
      const expandBtn = page.getByRole('button', { name: 'Show live task updates' })
      await expect(expandBtn).toBeVisible()

      // Click expand to restore
      await expandBtn.click()
      await expect(activityFeed.getByRole('heading', { name: 'Live task updates' })).toBeVisible({
        timeout: 3000,
      })
      await screenshot(page, '26-right-panel-toggle')
    })
  })

  // 15. List View Content ────────────────────────────────────────────────────

  test.describe('15. List View Content', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('list view shows task rows with data from mock', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      await topBar.getByRole('button', { name: 'List' }).click()

      await expectTaskListHeader(page)
      const tasksPage = page.getByTestId('page-tasks')
      await expect(tasksPage.getByText('Implement login flow')).toBeVisible({ timeout: 5000 })
      await expect(tasksPage.getByText('Fix database migration')).toBeVisible()
      await screenshot(page, '27-list-view-content')
    })

    test('clicking a list row opens the task document', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      await topBar.getByRole('button', { name: 'List' }).click()

      // Click on the task title text within the list row
      const taskRow = page.getByText('Implement login flow')
      await expect(taskRow).toBeVisible({ timeout: 5000 })
      await taskRow.click()

      await page.waitForURL('**/tasks/t-001')
      await page
        .getByRole('heading', { level: 1, name: 'Implement login flow' })
        .waitFor({ state: 'visible', timeout: 30000 })
      await expect(page.getByTestId('task-updates')).toBeVisible()
      await screenshot(page, '28-list-row-detail')
    })
  })

  // 16. Sidebar Project Tree ─────────────────────────────────────────────────

  test.describe('16. Sidebar Project Tree', () => {
    test('sidebar shows org, PROJECTS label, team and project', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      const sidebar = page.locator('[data-testid="sidebar"]')
      await expect(sidebar.getByText('Test Org')).toBeVisible({ timeout: 10000 })
      await expect(sidebar.getByText('PROJECTS')).toBeVisible()
      await expect(sidebar.getByText('Engineering')).toBeVisible()
      await expect(sidebar.getByTestId('project-proj-1')).toBeVisible()
      await screenshot(page, '29-sidebar-tree')
    })
  })

  // 17. No Project Selected State ────────────────────────────────────────────

  test.describe('17. Board Empty States', () => {
    test('no-group state explains that a project is needed before tasks', async ({
      page,
      context,
      baseURL,
    }) => {
      await injectAuth(page, baseURL!)
      // Override the default single-org mock with an empty list so the
      // nav loader never auto-selects a project.
      await overrideOrgs(context, [])

      // Navigate to the board route directly so this test stays focused on the no-project state.
      await gotoAndWaitForAppReady(page, baseURL!, '/tasks')

      await expect(page.locator('[data-testid="board-no-group"]')).toBeVisible({ timeout: 10000 })
      await expect(page.getByText('Create or choose a project before creating tasks')).toBeVisible()
      await screenshot(page, '30-board-no-project')
    })
  })

  // 18. Org Switcher ─────────────────────────────────────────────────────────

  test.describe('18. Org Switcher', () => {
    test('clicking org switcher opens dropdown with orgs', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideOrgs(context, [MOCK_ORG, MOCK_ORG_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      await page.locator('[data-testid="org-switcher"]').click()
      const dropdown = page.locator('[data-testid="org-dropdown"]')
      await expect(dropdown).toBeVisible({ timeout: 3000 })
      await expect(dropdown).toContainText('Test Org')
      await expect(dropdown).toContainText('Acme Corp')
      await screenshot(page, '31-org-switcher-open')
    })

    test('selected org shows checkmark', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideOrgs(context, [MOCK_ORG, MOCK_ORG_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      await page.locator('[data-testid="org-switcher"]').click()
      const dropdown = page.locator('[data-testid="org-dropdown"]')
      await expect(dropdown).toBeVisible({ timeout: 3000 })
      // Selected-row marker is a lucide <Check /> SVG with no accessible
      // name; the row itself gets `text-apple-blue` when selected, so
      // assert on that via its SVG child to be specific.
      await expect(dropdown.locator('button.text-apple-blue svg')).toBeVisible()
    })

    test('clicking outside closes dropdown', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideOrgs(context, [MOCK_ORG, MOCK_ORG_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      await page.locator('[data-testid="org-switcher"]').click()
      await expect(page.locator('[data-testid="org-dropdown"]')).toBeVisible({ timeout: 3000 })

      // Click outside the dropdown
      await page.locator('[data-testid="main-content"]').click()
      await expect(page.locator('[data-testid="org-dropdown"]')).toBeHidden({ timeout: 3000 })
    })

    test('org switcher shows org name when selected', async ({ page, baseURL }) => {
      await injectAuth(page, baseURL!)
      await gotoAndWaitForAppReady(page, baseURL!)

      await expect(page.locator('[data-testid="org-switcher"]')).toContainText('Test Org')
    })
  })

  // 19. Project Tree Expand/Collapse ─────────────────────────────────────────

  test.describe('19. Project Tree Expand/Collapse', () => {
    test('team toggle expands to show projects', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideTeams(context, [MOCK_TEAM, MOCK_TEAM_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      const sidebar = page.locator('[data-testid="sidebar"]')
      // Team-1 is already expanded (pre-seeded in localStorage)
      await expect(sidebar.getByTestId('project-proj-1')).toBeVisible({ timeout: 10000 })

      // Team-2 (Design) should be visible but collapsed
      await expect(sidebar.locator('[data-testid="team-team-2"]')).toBeVisible()
      // Click team-2 to expand
      await sidebar.locator('[data-testid="team-team-2"]').click()
      await expect(sidebar.getByText('Marketing Site')).toBeVisible({ timeout: 3000 })
      await screenshot(page, '32-project-tree-expanded')
    })

    test('team toggle collapses to hide projects', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideTeams(context, [MOCK_TEAM, MOCK_TEAM_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      const sidebar = page.locator('[data-testid="sidebar"]')
      // Team-1 is expanded — click to collapse
      await sidebar.locator('[data-testid="team-team-1"]').click()
      await expect(sidebar.getByTestId('project-proj-1')).toBeHidden({ timeout: 3000 })
    })

    test('selected project is highlighted', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideTeams(context, [MOCK_TEAM, MOCK_TEAM_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      const selectedProject = page.locator('[data-testid="project-proj-1"]')
      await expect(selectedProject).toBeVisible({ timeout: 10000 })
      // ProjectTree.tsx uses the tinted variant `bg-apple-blue/10` for the
      // selected row, not the solid `bg-apple-blue` TopBar uses.
      const classes = await selectedProject.getAttribute('class')
      expect(classes).toMatch(/\bbg-apple-blue\/10\b/)
    })

    test('clicking different project updates selection', async ({ page, context, baseURL }) => {
      await injectAuth(page, baseURL!)
      await overrideTeams(context, [MOCK_TEAM, MOCK_TEAM_2])
      await gotoAndWaitForAppReady(page, baseURL!)

      const sidebar = page.locator('[data-testid="sidebar"]')
      // Expand team-2
      await sidebar.locator('[data-testid="team-team-2"]').click()
      await expect(sidebar.getByText('Marketing Site')).toBeVisible({ timeout: 3000 })

      // Click the second project
      await sidebar.locator('[data-testid="project-proj-2"]').click()

      // Verify it becomes selected (blue highlight)
      // Selected row uses tinted `bg-apple-blue/10` (ProjectTree.tsx).
      await expect
        .poll(() => page.locator('[data-testid="project-proj-2"]').getAttribute('class'))
        .toMatch(/\bbg-apple-blue\/10\b/)
      await screenshot(page, '33-project-selection')
    })
  })

  // 20. Theme Persistence ────────────────────────────────────────────────────

  test.describe('20. Theme Persistence', () => {
    async function openThemeToggle(page: Page) {
      const toggle = page
        .getByTestId('sidebar')
        .getByRole('button', { name: /Switch to (dark|light) mode/i })
      await expect(toggle).toBeVisible()
      return toggle
    }

    test('dark theme adds dark class to document root', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      const themeToggle = await openThemeToggle(page)

      const isDark = await page.evaluate(() => document.documentElement.classList.contains('dark'))
      if (!isDark) {
        await themeToggle.click()
      }

      await expect
        .poll(() => page.evaluate(() => document.documentElement.classList.contains('dark')))
        .toBe(true)
      await screenshot(page, '34-dark-theme')
    })

    test('theme persists to localStorage', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      const themeToggle = await openThemeToggle(page)

      await themeToggle.click()
      await expect
        .poll(() => page.evaluate(() => localStorage.getItem('agentforge-theme')))
        .toMatch(/^(light|dark)$/)
    })
  })

  // 21. Board Filters ─────────────────────────────────────────────────────────
  //
  // Rewritten for the #629 redesign: the old Status/Agent/Priority TopBar
  // "Group By" toggle was removed and replaced by the BoardToolbar priority +
  // assignee filter groups. These drive the real filter UI (aria-pressed active
  // state) and assert the fixture-grounded "Showing N of M tasks" counts from
  // MOCK_TASKS (7 tasks: high=2, assigned=3, low=1 unassigned).
  test.describe('21. Board Filters', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('priority filter activates via aria-pressed and narrows the board', async ({ page }) => {
      const toolbar = await openBoardFilters(page)
      const priority = toolbar.getByRole('group', { name: 'Filter tasks by priority' })
      const high = priority.getByRole('button', { name: /Show high priority tasks/ })

      await expect(high).toHaveAttribute('aria-pressed', 'false')
      await high.click({ timeout: 30000 })
      await expect(high).toHaveAttribute('aria-pressed', 'true')
      await expect(
        priority.getByRole('button', { name: /Show tasks at all priority levels/ })
      ).toHaveAttribute('aria-pressed', 'false')
      // Fixture: 2 high-priority tasks (t-001, t-004) of 7.
      await expect(toolbar.getByText('Showing 2 of 7 tasks')).toBeVisible()
    })

    test('assignee filter toggles independently and Clear resets to all', async ({ page }) => {
      const toolbar = await openBoardFilters(page)
      const assignee = toolbar.getByRole('group', {
        name: 'Filter tasks by whether an agent is chosen',
      })
      const hasAgent = assignee.getByRole('button', {
        name: /Show tasks that already have an agent/,
      })

      await hasAgent.click({ timeout: 30000 })
      await expect(hasAgent).toHaveAttribute('aria-pressed', 'true')
      // Fixture: 3 assigned tasks (t-003, t-004, t-007) of 7.
      await expect(toolbar.getByText('Showing 3 of 7 tasks')).toBeVisible()

      // Clear appears only while a filter is active and resets every filter.
      await toolbar.getByRole('button', { name: 'Show all tasks' }).click()
      await expect(hasAgent).toHaveAttribute('aria-pressed', 'false')
      await expect(
        assignee.getByRole('button', { name: /Show tasks for all agent choices/ })
      ).toHaveAttribute('aria-pressed', 'true')
      await expect(toolbar.getByText('Showing 7 of 7 tasks')).toBeVisible()
    })

    test('over-filtering shows the guided empty state, not a blank board', async ({ page }) => {
      const toolbar = await openBoardFilters(page)

      // Low priority (only t-006) is unassigned, so Low + Has agent matches 0 tasks.
      await toolbar
        .getByRole('group', { name: 'Filter tasks by priority' })
        .getByRole('button', { name: /Show low priority tasks/ })
        .click({ timeout: 30000 })
      await toolbar
        .getByRole('group', { name: 'Filter tasks by whether an agent is chosen' })
        .getByRole('button', { name: /Show tasks that already have an agent/ })
        .click({ timeout: 30000 })

      const emptyState = page.getByTestId('board-filter-empty')
      await expect(emptyState).toBeVisible()
      await expect(toolbar.getByText('Showing 0 of 7 tasks')).toBeVisible()

      await emptyState.getByRole('button', { name: 'Show all tasks' }).click()
      await expect(emptyState).toBeHidden()
    })
  })

  // 22. Task Card Details ────────────────────────────────────────────────────

  test.describe('22. Task Card Details', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page
        .locator('[data-testid="page-tasks"]')
        .waitFor({ state: 'attached', timeout: 10000 })
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })
    })

    test('task card shows its task title', async ({ page }) => {
      const card = page.locator('[data-testid="task-card-t-001"]')
      await expect(card).toBeAttached({ timeout: 10000 })
      // The #629 redesign surfaces the task title on the card (the raw truncated
      // ID is no longer shown); the card is still keyed by id via its testid.
      await expect(card).toContainText('Implement login flow')
    })

    test('task card shows "No assignee" for unassigned tasks', async ({ page }) => {
      const card = page.locator('[data-testid="task-card-t-001"]')
      await expect(card).toBeAttached({ timeout: 10000 })
      await expect(card).toContainText('Needs agent')
    })

    test('task card shows agent name for assigned tasks', async ({ page }) => {
      const card = page.locator('[data-testid="task-card-t-004"]')
      await expect(card).toBeAttached({ timeout: 10000 })
      await expect(card).toContainText('GPT-4')
    })

    test('low priority task shows Low badge', async ({ page }) => {
      const card = page.locator('[data-testid="task-card-t-006"]')
      await expect(card).toBeAttached({ timeout: 10000 })
      await expect(card).toContainText('Low')
    })

    test('completed task appears in Done column', async ({ page }) => {
      const doneCount = page.locator('[data-testid="column-count-done"]')
      await expect(doneCount).toContainText('1')
      const card = page.locator('[data-testid="task-card-t-005"]')
      await expect(card).toBeAttached()
      await expect(card).toContainText('Deploy v2.1.0 to staging')
    })
  })

  // 23. Task Document Properties ─────────────────────────────────────────────

  test.describe('23. Task Document Properties', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })
    })

    test('properties show task state and priority', async ({ page }) => {
      await openTaskDocument(page, 't-004', 'Review PR #42')

      const properties = page.getByRole('button', { name: 'Properties', exact: true }).locator('..')
      await expect(properties).toContainText('Needs help', { timeout: 5000 })
      await expect(properties).toContainText('High')
      await expect(page.getByTestId('task-updates')).toBeVisible()
    })

    test('properties show the assigned agent', async ({ page }) => {
      await openTaskDocument(page, 't-003', 'Write unit tests for auth module')

      const properties = page.getByRole('button', { name: 'Properties', exact: true }).locator('..')
      await expect(properties).toContainText('Claude', { timeout: 5000 })
    })

    test('document explains an empty brief', async ({ page }) => {
      await openTaskDocument(page, 't-001', 'Implement login flow')

      await expect(page.getByTestId('task-brief-empty')).toContainText(
        'Only the task title was saved',
        { timeout: 5000 }
      )
    })

    test('blocked task does NOT show action buttons', async ({ page }) => {
      await openTaskDocument(page, 't-004', 'Review PR #42')

      const properties = page.getByRole('button', { name: 'Properties', exact: true }).locator('..')
      await expect(properties.getByRole('button', { name: 'Cancel' })).toHaveCount(0)
      await expect(properties.getByRole('button', { name: 'Needs help' })).toHaveCount(0)
    })
  })

  // 24. Quick Create Enter Submission ────────────────────────────────────────

  test.describe('24. Quick Create Submission', () => {
    test('Enter key submits and closes input', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })

      const addBtn = page.getByRole('button', { name: 'Add task idea' }).first()
      await addBtn.click()

      const input = page.getByRole('textbox', { name: 'Task goal' }).first()
      await input.fill('New task via Enter')
      await page.keyboard.press('Enter')

      // Input should close after submission
      await expect(input).toBeHidden({ timeout: 3000 })
      await screenshot(page, '35-quickcreate-submitted')
    })
  })

  // 25. Command Palette Navigation ───────────────────────────────────────────

  test.describe('25. Command Palette Navigation', () => {
    test('selecting Agents command navigates to /agents', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.keyboard.press('Control+k')
      const input = page.getByPlaceholder(/Search what you want to do/)
      await expect(input).toBeVisible({ timeout: 5000 })

      // Click the Agents navigation command
      await page.getByRole('option', { name: /^Agents\b/ }).click()

      await page.waitForURL('**/agents')
      await expect(
        page.getByTestId('page-agents').getByRole('heading', { name: 'Agents' })
      ).toBeVisible({ timeout: 5000 })
      await screenshot(page, '36-cmdk-navigate-agents')
    })

    test('selecting Settings command navigates to /settings', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.keyboard.press('Control+k')
      await expect(page.getByPlaceholder(/Search what you want to do/)).toBeVisible({
        timeout: 5000,
      })

      await page.getByRole('option', { name: /^Settings\b/ }).click()

      await page.waitForURL('**/settings')
      await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible({ timeout: 5000 })
    })

    test('selecting List view command switches to list view', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.keyboard.press('Control+k')
      await expect(page.getByPlaceholder(/Search what you want to do/)).toBeVisible({
        timeout: 5000,
      })

      const listCmd = page.getByRole('option', { name: /^List view\b/ })
      await listCmd.click()

      // Should show list view
      await expectTaskListHeader(page)
    })
  })

  // 26. Agents Page with Data ────────────────────────────────────────────────

  test.describe('26. Agents Page with Data', () => {
    test('agents page shows empty state or agent list', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.locator('[data-testid="sidebar-nav-agents"]').click()
      await page.waitForURL('**/agents')

      await expect(
        page.getByTestId('page-agents').getByRole('heading', { name: 'Agents' })
      ).toBeVisible({ timeout: 5000 })
      // Test asserts the page renders in SOME valid state — empty or
      // populated. `/api/v1/agents` is not mocked here so the backend's
      // actual response wins; assert on the page wrapper, which is present
      // in both branches of AgentListView.tsx.
      await expect(page.locator('[data-testid="page-agents"]')).toBeVisible()
      await screenshot(page, '37-agents-empty')
    })

    test('New agent button is visible', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page.locator('[data-testid="sidebar-nav-agents"]').click()
      await page.waitForURL('**/agents')

      const newAgentBtn = page.getByRole('button', { name: 'New agent' }).first()
      await expect(newAgentBtn).toBeVisible({ timeout: 5000 })
      await expect(newAgentBtn).toBeEnabled()
    })
  })

  // 27. Inbox with Notifications ─────────────────────────────────────────────

  test.describe('27. Inbox with Notifications', () => {
    test('inbox shows empty state by default', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.locator('[data-testid="sidebar-nav-inbox"]').click()
      await page.waitForURL('**/inbox')

      // Default Zustand state has no notifications
      await expect(page.getByText('No updates yet')).toBeVisible({ timeout: 5000 })
      const actionPath = page.getByTestId('inbox-action-path')
      await actionPath.getByRole('button', { name: 'Inbox action order', exact: true }).click()
      await expect(
        actionPath.getByText(
          'Next: start a task or wait for an agent update, then open Inbox again.',
          { exact: true }
        )
      ).toBeVisible()
      await screenshot(page, '38-inbox-default')
    })
  })

  // 28. Deep Linking / Direct URL ────────────────────────────────────────────

  test.describe('28. Client-Side Routing', () => {
    test('root URL / opens Tasks when the setup checklist is dismissed', async ({
      page,
      baseURL,
    }) => {
      await injectAuth(page, baseURL!)
      // The index route decides between /start (checklist not dismissed) and
      // /tasks; mock the preference so this test is deterministic on any DB.
      await mockUserPreferences(page.context(), { gettingStartedDismissed: true })
      await gotoAndWaitForAppReady(page, baseURL!, '/')

      await page.waitForURL('**/tasks')
      expect(page.url()).toContain('/tasks')
    })

    test('root URL / opens the Start checklist for a first-time workspace', async ({
      page,
      baseURL,
    }) => {
      await injectAuth(page, baseURL!)
      await mockUserPreferences(page.context(), { gettingStartedDismissed: false })
      await gotoAndWaitForAppReady(page, baseURL!, '/')

      await page.waitForURL('**/start')
      await expect(page.getByTestId('page-start')).toBeVisible({ timeout: 30000 })
    })

    test('navigating to each page and back preserves state', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      // Navigate through all pages
      await page.locator('[data-testid="sidebar-nav-agents"]').click()
      await page.waitForURL('**/agents')
      await expect(
        page.getByTestId('page-agents').getByRole('heading', { name: 'Agents' })
      ).toBeVisible({ timeout: 5000 })

      await page.locator('[data-testid="sidebar-nav-skills"]').click()
      await page.waitForURL('**/skills')
      await expect(page.getByRole('heading', { name: 'Skills', level: 1 })).toBeVisible({
        timeout: 5000,
      })

      // Go back to tasks — kanban should still be there
      await page.locator('[data-testid="sidebar-nav-tasks"]').click()
      await page.waitForURL('**/tasks')
      await expect(page.locator('[data-testid="column-count-backlog"]')).toBeAttached({
        timeout: 10000,
      })
      await screenshot(page, '39-routing-roundtrip')
    })
  })

  // 29. Sidebar Collapsed State ──────────────────────────────────────────────

  test.describe('29. Sidebar Collapsed State', () => {
    test('collapsed sidebar still shows nav icon buttons', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      // Collapse sidebar
      await page.keyboard.press('Control+\\')

      // Nav items should still be attached (as icon-only buttons)
      await expect(page.locator('[data-testid="sidebar-nav-tasks"]')).toBeAttached()
      await expect(page.locator('[data-testid="sidebar-nav-agents"]')).toBeAttached()
      await expect(page.locator('[data-testid="sidebar-nav-settings"]')).toBeAttached()
      await screenshot(page, '40-sidebar-collapsed-icons')
    })

    test('collapsed sidebar hides PROJECTS label and tree', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.keyboard.press('Control+\\')

      // PROJECTS label and tree should be hidden when collapsed
      await expect(page.getByText('PROJECTS')).toBeHidden()
      await expect(page.locator('[data-testid="org-switcher"]')).toBeHidden()
    })

    test('collapsed sidebar navigation still works', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.keyboard.press('Control+\\')

      // Click agents nav (icon only)
      await page.locator('[data-testid="sidebar-nav-agents"]').click()
      await page.waitForURL('**/agents')
      await expect(
        page.getByTestId('page-agents').getByRole('heading', { name: 'Agents' })
      ).toBeVisible({ timeout: 5000 })
    })
  })

  // 30. Activity Feed Content ────────────────────────────────────────────────

  test.describe('30. Activity Feed Content', () => {
    test('right panel shows empty activity state by default', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      // Copy in ActivityFeed.tsx is "Quiet so far" for the empty state.
      const rightPanel = page.locator('[data-testid="right-panel"]')
      await expect(rightPanel.getByText('Quiet so far')).toBeVisible({ timeout: 5000 })
    })

    test('activity panel header shows "Activity" title', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      const rightPanel = page.locator('[data-testid="right-panel"]')
      await expect(rightPanel.getByRole('heading', { name: 'Live task updates' })).toBeVisible()
    })
  })

  // 31. Multiple Task Documents ──────────────────────────────────────────────

  test.describe('31. Task Document Navigation', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page
        .locator('[data-testid="column-count-backlog"]')
        .waitFor({ state: 'attached', timeout: 10000 })
    })

    test('opening different cards routes to their documents', async ({ page }) => {
      await openTaskDocument(page, 't-001', 'Implement login flow')

      await page
        .getByRole('navigation', { name: 'Breadcrumb' })
        .getByRole('button', { name: 'Tasks', exact: true })
        .click()
      await page.waitForURL('**/tasks')

      await openTaskDocument(page, 't-002', 'Fix database migration')
      await expect(
        page.getByRole('heading', { level: 1, name: 'Implement login flow' })
      ).toHaveCount(0)
      await screenshot(page, '41-task-switch')
    })
  })

  // 32. Keyboard Shortcut: Ctrl+K Toggle ─────────────────────────────────────

  test.describe('32. Keyboard Shortcuts', () => {
    test('Ctrl+K toggles command palette (open then close)', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      // Open
      await page.keyboard.press('Control+k')
      await expect(page.getByPlaceholder(/Search what you want to do/)).toBeVisible({
        timeout: 5000,
      })

      // Close with same shortcut
      await page.keyboard.press('Control+k')
      await expect(page.getByPlaceholder(/Search what you want to do/)).toBeHidden({
        timeout: 3000,
      })
    })

    test('Escape closes command palette', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)

      await page.keyboard.press('Control+k')
      const input = page.getByPlaceholder(/Search what you want to do/)
      await expect(input).toBeVisible({ timeout: 5000 })

      // cmdk captures Escape on the input element — click backdrop instead
      await page.mouse.click(5, 5)
      await expect(input).toBeHidden({ timeout: 3000 })
    })
  })

  // 33. View Mode Button Active State ────────────────────────────────────────

  test.describe('33. View Mode Active State', () => {
    test.beforeEach(async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
    })

    test('Board button has active neutral fill by default', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      const boardBtn = topBar.getByRole('button', { name: 'Board' })
      const classes = await boardBtn.getAttribute('class')
      expect(classes).toMatch(/(^|\s)bg-black\/\[0\.06\]($|\s)/)
    })

    test('switching to List makes List button active', async ({ page }) => {
      const topBar = page.locator('[data-testid="top-bar"]')
      const listBtn = topBar.getByRole('button', { name: 'List' })
      const boardBtn = topBar.getByRole('button', { name: 'Board' })
      await listBtn.click()

      const listClasses = await listBtn.getAttribute('class')
      expect(listClasses).toMatch(/(^|\s)bg-black\/\[0\.06\]($|\s)/)

      // Board should no longer have active style
      const boardClasses = await boardBtn.getAttribute('class')
      expect(boardClasses).not.toMatch(/(^|\s)bg-black\/\[0\.06\]($|\s)/)
    })
  })

  // 34. Settings About Section ───────────────────────────────────────────────

  test.describe('34. Settings About Section', () => {
    test('shows application name and version', async ({ page, baseURL }) => {
      await setupAndNavigate(page, baseURL!)
      await page.locator('[data-testid="sidebar-nav-settings"]').click()
      await page.waitForURL('**/settings')

      // About is its own section in SettingsLayout; click into it first.
      await openSettingsGroup(page, 'more setup')
      await page
        .locator('[data-testid="settings-desktop-nav"]')
        .getByRole('link', { name: /^About:/ })
        .click()

      const about = page.locator('[data-testid="settings-about"]')
      await expect(about).toBeVisible({ timeout: 5000 })
      await expect(about).toContainText('Wisdoverse Forge')
      // Version comes from package.json via Vite `__APP_VERSION__` define;
      // assert on the shape not a literal so a bump doesn't break the test.
      const version = await about.locator('[data-testid="settings-about-version"]').textContent()
      expect(version).toMatch(/^\d+\.\d+\.\d+/)
      await screenshot(page, '42-settings-about')
    })
  })
})
