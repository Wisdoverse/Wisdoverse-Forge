import { expect, test, type Page, type Response } from '@playwright/test'

// Opt in with a project in a disposable local stack. Uses canonical real login,
// live Rust routes and PostgreSQL; GitHub may be a local provider test server.
const projectId = process.env.E2E_MAINTENANCE_PROJECT_ID
const orgId = process.env.E2E_MAINTENANCE_ORG_ID
const teamId = process.env.E2E_MAINTENANCE_TEAM_ID
const providerControl = process.env.E2E_MAINTENANCE_PROVIDER_CONTROL

async function openIntake(page: Page) {
  await page.addInitScript(
    ({ orgId, projectId, teamId }) => {
      localStorage.setItem('af:nav:orgId', orgId!)
      localStorage.setItem('af:nav:projectId', projectId!)
      localStorage.setItem('af:nav:expandedTeams', JSON.stringify([teamId]))
    },
    { orgId, projectId, teamId }
  )
  await page.goto('/tasks', { waitUntil: 'domcontentloaded' })
  const intake = page.getByTestId('maintenance-intake')
  await intake.waitFor({ state: 'visible', timeout: 30_000 })
  await intake
    .getByRole('button', { name: 'Maintenance request', exact: true })
    .click({ timeout: 30_000 })
  await expect(intake.getByRole('button', { name: 'Save maintenance task to wait' })).toBeEnabled()
  return intake
}

async function createMaintenanceTask(page: Page, title: string) {
  const intake = await openIntake(page)
  const groupId = await page.getByRole('combobox', { name: 'Place for new tasks' }).inputValue()
  const reference = `board-${crypto.randomUUID()}`
  await intake.getByLabel('Stable request reference').fill(reference)
  await intake.getByLabel('Maintenance title').fill(title)
  await intake.getByLabel('Maintenance brief').fill('Review this task through the real board API.')
  const submission = page.waitForResponse(
    (response) =>
      response.url().endsWith('/api/v1/self-fix/requests') && response.request().method() === 'POST'
  )
  await intake.getByRole('button', { name: 'Save maintenance task to wait' }).click()
  const response = await submission
  expect(response.ok(), `Maintenance submission status ${response.status()}`).toBe(true)
  return { taskId: (await response.json()).data.taskId as string, groupId }
}

function taskListResponse(response: Response, groupId: string) {
  return (
    response.request().method() === 'GET' &&
    new URL(response.url()).pathname.endsWith(`/api/v1/orchestration/groups/${groupId}/tasks`)
  )
}

test.describe('maintenance browser to API workflow', () => {
  test.skip(
    !projectId || !orgId || !teamId,
    'Requires an explicitly supplied disposable maintenance project and platform administrator.'
  )
  test.beforeEach(async ({ baseURL }) => {
    expect(['localhost', '127.0.0.1', '[::1]']).toContain(new URL(baseURL!).hostname)
    if (providerControl)
      expect(['localhost', '127.0.0.1', '[::1]']).toContain(new URL(providerControl).hostname)
  })

  test('creates waiting work, reuses its source and opens the original task trace', async ({
    page,
  }) => {
    const intake = await openIntake(page)
    const reference = `browser-${crypto.randomUUID()}`
    await intake.getByLabel('Stable request reference').fill(reference)
    await intake.getByLabel('Maintenance title').fill('Review a dependency update')
    await intake
      .getByLabel('Maintenance brief')
      .fill('Inspect the dependency and report the checks needed before assigning work.')
    const submission = page.waitForResponse(
      (response) =>
        response.url().endsWith('/api/v1/self-fix/requests') &&
        response.request().method() === 'POST'
    )
    await intake.getByRole('button', { name: 'Save maintenance task to wait' }).click()
    const created = await submission
    expect(
      created.ok(),
      `Maintenance submission status ${created.status()}: ${await created.text()}`
    ).toBe(true)
    const first = (await created.json()).data as {
      taskId: string
      requestId: string
      reused: boolean
    }
    expect(first.reused).toBe(false)
    await expect(intake.getByRole('status')).toContainText('saved to wait')
    await intake.getByRole('button', { name: 'Submit another source' }).click()
    await intake.getByLabel('Stable request reference').fill(reference.toUpperCase())
    await intake
      .getByLabel('Maintenance title')
      .fill('This retry must not replace the original title')
    await intake
      .getByLabel('Maintenance brief')
      .fill('This retry must keep the original task brief.')
    const replay = page.waitForResponse(
      (response) =>
        response.url().endsWith('/api/v1/self-fix/requests') &&
        response.request().method() === 'POST'
    )
    await intake.getByRole('button', { name: 'Save maintenance task to wait' }).click()
    expect((await (await replay).json()).data).toEqual({ ...first, reused: true })
    await expect(intake.getByRole('status')).toContainText(
      'original brief and destination were kept'
    )
    await intake.getByRole('button', { name: 'Open maintenance task' }).click()
    await expect(page).toHaveURL(new RegExp(`/tasks/${first.taskId}$`))
    await expect(
      page.getByRole('heading', { level: 1, name: 'Review a dependency update', exact: true })
    ).toHaveText('Review a dependency update')
    await expect(
      page.getByText('Inspect the dependency and report the checks needed before assigning work.', {
        exact: true,
      })
    ).toBeVisible()
    const trace = page.getByTestId('maintenance-trace')
    await expect(trace.getByText(`Request ${reference}`, { exact: true })).toBeVisible()
    await expect(trace).toContainText('No execution recorded')
    await expect(trace).toContainText('No produced PR recorded')
    await expect(trace).toContainText('backlog')
    await expect(trace).toContainText('review those separately before using a change')
    expect(await trace.locator('time').first().getAttribute('datetime')).toMatch(
      /^\d{4}-\d{2}-\d{2}T/
    )
  })

  test('updates priority through the real board API and reads it back on a narrow keyboard path', async ({
    page,
  }) => {
    const title = `Board priority ${crypto.randomUUID()}`
    const { taskId, groupId } = await createMaintenanceTask(page, title)
    const readbackRequest = page.waitForResponse((response) => taskListResponse(response, groupId))
    await page.reload()
    const readback = await readbackRequest
    expect(readback.ok()).toBe(true)
    const initial = (await readback.json()).tasks.find((task: { id: string }) => task.id === taskId)
    expect(initial?.priority).toBe('normal')

    await page.setViewportSize({ width: 390, height: 844 })
    const priority = page.getByTestId(`task-priority-${taskId}`)
    await expect(priority).toHaveValue('normal')
    await priority.focus()
    const update = page.waitForResponse(
      (response) =>
        response.url().endsWith(`/api/v1/orchestration/tasks/${taskId}`) &&
        response.request().method() === 'PATCH'
    )
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    const response = await update
    expect(response.ok(), `Task update status ${response.status()}`).toBe(true)
    expect(response.request().postDataJSON()).toEqual({ priority: 'high' })
    expect((await response.json()).task).toMatchObject({ id: taskId, priority: 'high' })

    const persistedReadback = page.waitForResponse((candidate) =>
      taskListResponse(candidate, groupId)
    )
    await page.reload()
    const persisted = await persistedReadback
    expect(persisted.ok()).toBe(true)
    const updated = (await persisted.json()).tasks.find(
      (task: { id: string }) => task.id === taskId
    )
    expect(updated?.priority).toBe('high')
    await expect(page.getByTestId(`task-priority-${taskId}`)).toHaveValue('high')
  })

  test('keeps cards during a failed background refresh and retries against the real API', async ({
    page,
  }) => {
    await page.clock.install()
    const title = `Board stale refresh ${crypto.randomUUID()}`
    const { taskId, groupId } = await createMaintenanceTask(page, title)
    await page.reload()
    const card = page.getByTestId(`task-card-${taskId}`)
    await expect(card).toBeVisible()

    const targetPath = `/api/v1/orchestration/groups/${groupId}/tasks`
    let shouldAbort = true
    let aborted = false
    await page.route(`**${targetPath}`, async (route) => {
      if (shouldAbort && route.request().method() === 'GET' && !aborted) {
        aborted = true
        await route.abort()
        return
      }
      await route.continue()
    })
    await page.clock.fastForward(30_001)
    const stale = page.getByTestId('board-stale-refresh')
    await expect(stale).toContainText('Could not refresh tasks. Showing the last loaded cards.')
    await expect(card).toBeVisible()

    shouldAbort = false
    const retry = page.waitForResponse((response) => taskListResponse(response, groupId))
    await stale.getByRole('button', { name: 'Check tasks again' }).click()
    const refreshed = await retry
    expect(refreshed.ok()).toBe(true)
    expect((await refreshed.json()).tasks.some((task: { id: string }) => task.id === taskId)).toBe(
      true
    )
    await expect(page.getByTestId('board-stale-refresh')).toHaveCount(0)
    await expect(card).toBeVisible()
  })

  test('shows submitted and changed PR versions plus provider outage on a narrow screen', async ({
    page,
    request,
  }) => {
    test.skip(
      !providerControl,
      'Requires a local GitHub provider with a test/state control endpoint.'
    )
    await request.post(`${providerControl}/test/state`, {
      data: { head: 'a'.repeat(40), available: true },
    })
    await page.setViewportSize({ width: 390, height: 844 })
    const intake = await openIntake(page)
    await intake.getByLabel('Source type').selectOption('pull_request')
    await intake.getByRole('textbox', { name: /^GitHub PR number/ }).fill('42')
    await intake.getByLabel('Maintenance title').fill('Inspect the failing pull request')
    await intake
      .getByLabel('Maintenance brief')
      .fill('Review the current revision before assigning repair work.')
    const submitted = page.waitForResponse(
      (response) =>
        response.url().endsWith('/api/v1/self-fix/requests') &&
        response.request().method() === 'POST'
    )
    await intake.getByRole('button', { name: 'Save maintenance task to wait' }).click()
    expect((await submitted).ok()).toBe(true)
    await intake.getByRole('button', { name: 'Open maintenance task' }).click()
    const trace = page.getByTestId('maintenance-trace')
    await expect(
      trace.getByRole('heading', { name: 'Maintenance source and result' })
    ).toBeVisible()
    const submittedSourceVersion = trace
      .getByText('Source PR version at submission', { exact: true })
      .locator('..')
      .locator('dd')
    const currentSourcePr = trace.getByRole('heading', { name: 'Current source PR' }).locator('..')
    const observedSourceVersion = currentSourcePr
      .getByText('Observed current version', { exact: true })
      .locator('..')
      .locator('dd')
    await expect(submittedSourceVersion).toHaveText('a'.repeat(40))
    await expect(observedSourceVersion).toHaveText('a'.repeat(40))
    await request.post(`${providerControl}/test/state`, {
      data: { head: 'c'.repeat(40), available: true },
    })
    await trace.getByRole('button', { name: 'Refresh source and result' }).focus()
    await page.keyboard.press('Enter')
    await expect(trace).toContainText('The PR version changed')
    await expect(submittedSourceVersion).toHaveText('a'.repeat(40))
    await expect(observedSourceVersion).toHaveText('c'.repeat(40))
    expect(await trace.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true)
    await request.post(`${providerControl}/test/state`, { data: { available: false } })
    await trace.getByRole('button', { name: 'Refresh source and result' }).click()
    await expect(trace).toContainText('GitHub state is unavailable')
    await expect(submittedSourceVersion).toHaveText('a'.repeat(40))
    await expect(currentSourcePr.getByText('c'.repeat(40), { exact: true })).toHaveCount(0)
    await expect(trace.getByRole('button', { name: /approve|merge/i })).toHaveCount(0)
    await request.post(`${providerControl}/test/state`, { data: { available: true } })
  })
})
