import { expect, test, type Page } from '@playwright/test'

// Explicit disposable fixtures contain synthetic finished-run metadata. This
// proves browser -> real login -> Rust -> PostgreSQL records, not vendor runs.
const taskId = process.env.E2E_MAINTENANCE_DELIVERY_TASK_ID
const otherTaskId = process.env.E2E_MAINTENANCE_DELIVERY_OTHER_TASK_ID
const orgId = process.env.E2E_MAINTENANCE_ORG_ID,
  projectId = process.env.E2E_MAINTENANCE_PROJECT_ID,
  teamId = process.env.E2E_MAINTENANCE_TEAM_ID
const provider = process.env.E2E_MAINTENANCE_PROVIDER_CONTROL

async function openTask(page: Page, id: string) {
  await page.addInitScript(
    ({ orgId, projectId, teamId }) => {
      localStorage.setItem('af:nav:orgId', orgId!)
      localStorage.setItem('af:nav:projectId', projectId!)
      localStorage.setItem('af:nav:expandedTeams', JSON.stringify([teamId]))
    },
    { orgId, projectId, teamId }
  )
  await page.goto(`/tasks/${id}`, { waitUntil: 'domcontentloaded' })
  const panel = page.getByTestId('maintenance-delivery')
  await expect(panel.getByRole('button', { name: 'Refresh evidence' })).toBeEnabled({
    timeout: 30_000,
  })
  return panel
}
async function saveReport(page: Page, id: string) {
  const panel = await openTask(page, id)
  await panel.getByText('Create verification report', { exact: true }).click()
  await panel.getByLabel('Allowed scope', { exact: true }).fill('Dependency constraints only.')
  await panel.getByLabel('Acceptance criteria', { exact: true }).fill('The dependency tests pass.')
  await panel
    .getByLabel('Environment and constraints', { exact: true })
    .fill('Disposable browser rehearsal; run metadata is synthetic.')
  await panel
    .getByLabel('Changes and known failures', { exact: true })
    .fill(
      'A synthetic recorded change exercises the review workflow; vendor execution remains unverified.'
    )
  await panel
    .getByLabel('Unverified areas (one per line, up to 20)', { exact: true })
    .fill('Actual Container CLI execution\nProduction deployment')
  await panel.getByText('Optional Container CLI comparison', { exact: true }).click()
  await panel
    .getByLabel('Reported Container CLI version', { exact: true })
    .fill('synthetic-rehearsal-metadata')
  await panel.getByLabel('Comparison reference', { exact: true }).fill('browser-equivalent-task')
  const response = page.waitForResponse(
    (r) => r.url().endsWith(`/self-fix/tasks/${id}/reports`) && r.request().method() === 'POST'
  )
  await panel.getByRole('button', { name: 'Save verification report' }).click()
  const saved = await response
  expect(saved.ok(), await saved.text()).toBe(true)
  await expect(panel.getByRole('button', { name: 'Refresh evidence' })).toBeEnabled({
    timeout: 30_000,
  })
  return panel
}

test.describe('maintenance delivery browser rehearsal', () => {
  test.skip(
    !taskId || !otherTaskId || !orgId || !projectId || !teamId || !provider,
    'Requires explicitly provisioned disposable finished-run fixtures and a local provider control.'
  )
  test.beforeEach(async ({ baseURL, request }) => {
    expect(['localhost', '127.0.0.1', '[::1]']).toContain(new URL(baseURL!).hostname)
    expect(['localhost', '127.0.0.1', '[::1]']).toContain(new URL(provider!).hostname)
    await request.post(`${provider}/test/state`, {
      data: { available: true, producedHead: 'c'.repeat(40) },
    })
  })
  test('saves revision-bound reports, human effort and a retained handoff on a narrow screen', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 844 })
    const writes: string[] = []
    page.on('request', (r) => {
      if (r.method() === 'POST' && /\/(approve|merge)$/.test(r.url())) writes.push(r.url())
    })
    const panel = await saveReport(page, taskId!)
    await panel.getByText('Record human verdict and effort', { exact: true }).click()
    await panel.getByLabel('Human verdict', { exact: true }).selectOption('accepted')
    await panel
      .getByLabel('Verdict reason and quality findings', { exact: true })
      .fill(
        'Accepted this synthetic workflow record after reviewing its exact revision; actual execution remains unverified.'
      )
    for (const [category, value] of Object.entries({
      setup: 0,
      handling: 2,
      review: 3,
      recovery: 1,
      rework: 0,
      operation: 0,
    }))
      await panel.getByLabel(`${category} minutes`, { exact: true }).fill(String(value))
    await panel
      .getByLabel('Comparable baseline minutes (optional, total for equivalent human work)', {
        exact: true,
      })
      .fill('12')
    const response = page.waitForResponse(
      (r) =>
        r.url().endsWith(`/self-fix/tasks/${taskId}/decisions`) && r.request().method() === 'POST'
    )
    await panel.getByRole('button', { name: 'Save human verdict' }).click()
    const saved = await response
    expect(saved.ok(), await saved.text()).toBe(true)
    expect((await saved.json()).data.totalMinutes).toBe(6)
    await expect(panel.getByRole('button', { name: 'Refresh evidence' })).toBeEnabled({
      timeout: 30_000,
    })
    await panel.getByText('Record handoff for a person to continue', { exact: true }).click()
    await panel
      .getByLabel('Handoff blocker or reason', { exact: true })
      .fill('Continue the real runtime validation outside this synthetic rehearsal.')
    await panel
      .getByLabel('Next action for the person continuing', { exact: true })
      .fill('Run equivalent tasks with real supported Container CLIs, then review their artifacts.')
    await panel.getByRole('button', { name: 'Save handoff' }).click()
    await expect(panel.getByTestId('maintenance-handoff').first()).toContainText(
      'Continue the real runtime validation'
    )
    await expect(panel.getByTestId('verification-report').first()).toContainText('6 minutes')
    expect(await panel.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true)
    expect(writes).toEqual([])
  })
  test('compares recorded conditions and disables acceptance after head drift or outage', async ({
    page,
    request,
  }) => {
    await saveReport(page, otherTaskId!)
    await page.goto('/analytics', { waitUntil: 'domcontentloaded' })
    const outcomes = page.getByTestId('maintenance-outcomes')
    await expect(outcomes.getByRole('button', { name: 'Refresh outcomes' })).toBeEnabled({
      timeout: 30_000,
    })
    for (const id of [taskId!, otherTaskId!])
      await outcomes
        .locator('tr')
        .filter({ has: page.locator(`a[href="/tasks/${id}"]`) })
        .getByRole('checkbox')
        .check()
    await outcomes.getByRole('button', { name: 'Compare selected reports (2/8)' }).click()
    await expect(outcomes.getByTestId('maintenance-comparison')).toContainText(
      'Recorded comparison conditions match'
    )
    await expect(outcomes.getByTestId('maintenance-comparison')).toContainText(
      '2 distinct recorded Container CLIs'
    )
    const panel = await openTask(page, otherTaskId!)
    await request.post(`${provider}/test/state`, { data: { producedHead: 'd'.repeat(40) } })
    await panel.getByRole('button', { name: 'Refresh evidence' }).click()
    await expect(panel).toContainText('The current PR head differs')
    await panel.getByText('Record human verdict and effort', { exact: true }).click()
    await expect(
      panel.getByLabel('Human verdict', { exact: true }).locator('option[value="accepted"]')
    ).toBeDisabled()
    await request.post(`${provider}/test/state`, { data: { available: false } })
    await panel.getByRole('button', { name: 'Refresh evidence' }).click()
    await expect(panel).toContainText('GitHub could not be refreshed')
    await expect(panel.getByTestId('verification-report').first()).toContainText(
      'vendor execution remains unverified'
    )
    await request.post(`${provider}/test/state`, {
      data: { available: true, producedHead: 'c'.repeat(40) },
    })
  })
})
