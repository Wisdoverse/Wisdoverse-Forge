import { expect, test, type Page } from '@playwright/test'

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
    await expect(trace.getByText('a'.repeat(40), { exact: true })).toHaveCount(2)
    await request.post(`${providerControl}/test/state`, {
      data: { head: 'c'.repeat(40), available: true },
    })
    await trace.getByRole('button', { name: 'Refresh source and result' }).focus()
    await page.keyboard.press('Enter')
    await expect(trace).toContainText('The PR version changed')
    await expect(trace.getByText('a'.repeat(40), { exact: true })).toHaveCount(1)
    await expect(trace.getByText('c'.repeat(40), { exact: true })).toBeVisible()
    expect(await trace.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true)
    await request.post(`${providerControl}/test/state`, { data: { available: false } })
    await trace.getByRole('button', { name: 'Refresh source and result' }).click()
    await expect(trace).toContainText('GitHub state is unavailable')
    await expect(trace.getByText('a'.repeat(40), { exact: true })).toBeVisible()
    await expect(trace.getByText('c'.repeat(40), { exact: true })).toHaveCount(0)
    await expect(trace.getByRole('button', { name: /approve|merge/i })).toHaveCount(0)
    await request.post(`${providerControl}/test/state`, { data: { available: true } })
  })
})
