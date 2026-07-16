import type { BrowserContext, Page, Route } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'
import { MOCK_TASKS, makeMockTask } from '../fixtures/mocks'

const DOCUMENT_TASK = {
  ...makeMockTask('t-003', 'Write unit tests for auth module', 'working', 'normal', 45, 'Claude'),
  params: {
    task: 'Write unit tests for auth module',
    message: '## Acceptance\n\nPreserve the board filters and return to the same task list.',
  },
}

const DOCUMENT_TASKS = MOCK_TASKS.map((task) =>
  task.id === DOCUMENT_TASK.id ? DOCUMENT_TASK : task
)

function json(route: Route, body: unknown, status = 200): Promise<void> {
  return route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body),
  })
}

async function installTaskDocumentMocks(context: BrowserContext): Promise<void> {
  await context.route('**/api/v1/orchestration/groups/*/tasks*', (route) => {
    const request = route.request()
    const pathname = new URL(request.url()).pathname
    if (request.method() !== 'GET' || pathname.endsWith('/stats')) return route.fallback()
    return json(route, { ok: true, tasks: DOCUMENT_TASKS })
  })

  await context.route('**/api/v1/orchestration/tasks/*', (route) => {
    const request = route.request()
    const pathname = new URL(request.url()).pathname
    if (request.method() !== 'GET') return route.fallback()
    if (pathname === `/api/v1/orchestration/tasks/${DOCUMENT_TASK.id}/runs`) {
      return json(route, { ok: true, runs: [] })
    }
    if (pathname === `/api/v1/orchestration/tasks/${DOCUMENT_TASK.id}`) {
      return json(route, { ok: true, task: DOCUMENT_TASK })
    }
    if (/\/api\/v1\/orchestration\/tasks\/[^/]+$/.test(pathname)) {
      return json(route, { ok: false, error: 'Task not found' }, 404)
    }
    return route.fallback()
  })
}

async function seedNavigation(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
}

async function waitForDocument(page: Page): Promise<void> {
  await page
    .getByRole('heading', { level: 1, name: DOCUMENT_TASK.params.task })
    .waitFor({ state: 'visible', timeout: 30000 })
  await expect(
    page.getByText('Preserve the board filters and return to the same task list.')
  ).toBeVisible({ timeout: 30000 })
}

test.describe('Task document page', () => {
  test.beforeEach(async ({ context, page }) => {
    await installTaskDocumentMocks(context)
    await seedNavigation(page)
  })

  test('opens from a card and returns to the board through the breadcrumb', async ({ page }) => {
    await page.goto('/tasks', { waitUntil: 'domcontentloaded' })
    const toolbar = page.getByTestId('board-toolbar')
    await toolbar.waitFor({ state: 'visible', timeout: 30000 })
    await toolbar.getByRole('button', { name: /^Filters/ }).click({ timeout: 30000 })
    await expect(
      toolbar.getByRole('group', { name: 'Filter tasks by whether an agent is chosen' })
    ).toBeVisible()

    const card = page.getByTestId(`task-card-${DOCUMENT_TASK.id}`)
    await card.waitFor({ state: 'visible', timeout: 30000 })
    await card.dispatchEvent('click')

    await page.waitForURL(`**/tasks/${DOCUMENT_TASK.id}`)
    await waitForDocument(page)

    const breadcrumb = page.getByRole('navigation', { name: 'Breadcrumb' })
    await breadcrumb.getByRole('button', { name: 'Tasks', exact: true }).click()

    await page.waitForURL('**/tasks')
    await expect(page.getByTestId('page-tasks')).toBeVisible({ timeout: 30000 })
    await expect(page.getByTestId(`task-card-${DOCUMENT_TASK.id}`)).toBeVisible()
    await expect(
      page.getByTestId('board-toolbar').getByRole('button', { name: /^Filters/ })
    ).toBeVisible()
  })

  test('resolves a cold deep link', async ({ page }) => {
    await page.goto(`/tasks/${DOCUMENT_TASK.id}`, { waitUntil: 'domcontentloaded' })

    await waitForDocument(page)
    await expect(page.getByTestId('task-updates')).toBeVisible()
  })

  test('shows the missing-task path for an unknown id', async ({ page }) => {
    await page.goto('/tasks/missing-task', { waitUntil: 'domcontentloaded' })

    await page
      .getByRole('heading', { level: 1, name: 'This task is not on the board anymore.' })
      .waitFor({ state: 'visible', timeout: 30000 })
    await expect(page.getByRole('button', { name: 'Open the task board' })).toBeVisible()
  })
})
