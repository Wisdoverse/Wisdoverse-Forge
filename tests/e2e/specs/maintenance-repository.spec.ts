import { expect, test, type Locator, type Page } from '@playwright/test'
import type { SelfFixRepositorySetup } from '../../../shared/types/self-fix'

// Use the canonical real-login setup and live Rust routes. Do not replace auth
// or repository responses with browser mocks. GitHub may be a local test server.
async function openRepositorySettings(page: Page): Promise<{ section: Locator; isAdmin: boolean }> {
  await page.goto('/settings/maintenance-repository', { waitUntil: 'domcontentloaded' })
  const section = page.getByTestId('settings-maintenance-repository')
  await expect(section).toBeVisible()
  const isAdmin = await page.evaluate(async () => {
    const token = localStorage.getItem('af:auth:access')
    const response = await fetch('/api/v1/me', {
      headers: { Authorization: `Bearer ${token}` },
      credentials: 'include',
    })
    if (!response.ok) throw new Error('Real account access could not be verified')
    return (await response.json()).isAdmin === true
  })
  return { section, isAdmin }
}

function field(section: Locator, label: string): Locator {
  return section.getByText(label, { exact: true }).locator('..').locator('dd')
}

test('checks the approved repository with real authentication and keyboard refresh', async ({
  page,
}) => {
  const methods: string[] = []
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/v1/self-fix/repository') {
      methods.push(request.method())
    }
  })
  const { section, isAdmin } = await openRepositorySettings(page)
  if (!isAdmin) {
    await expect(section.getByRole('status')).toContainText('Ask a Forge administrator')
    await expect(section.getByRole('button')).toHaveCount(0)
    await expect(section.getByTestId('maintenance-repository-snapshot')).toHaveCount(0)
    expect(methods).toEqual([])
    return
  }

  const check = section.getByRole('button', { name: 'Check connection', exact: true })
  await expect(check).toBeEnabled({ timeout: 35_000 })
  await check.focus()
  await expect(check).toBeFocused()
  const responsePromise = page.waitForResponse(
    (response) => new URL(response.url()).pathname === '/api/v1/self-fix/repository'
  )
  await page.keyboard.press('Enter')
  const response = await responsePromise
  await expect(check).toBeEnabled({ timeout: 35_000 })
  if (response.ok()) {
    const snapshot = ((await response.json()) as { data: SelfFixRepositorySetup }).data
    await expect(field(section, 'Approved repository')).toHaveText(snapshot.repository)
    await expect(field(section, 'Default branch')).toHaveText(snapshot.defaultBranch)
    await expect(field(section, 'Starting version')).toHaveText(snapshot.baseSha)
    const flags = [
      snapshot.contentsWrite,
      snapshot.pullRequestsWrite,
      snapshot.checksRead,
      snapshot.squashMergeAllowed,
    ]
    const rows = section.locator('li')
    await expect(rows).toHaveCount(4)
    for (let index = 0; index < flags.length; index++) {
      await expect(rows.nth(index)).toContainText(flags[index] ? 'Available' : 'Needs setup')
    }
    await expect(section).toContainText('Each change still needs its own checks and human review')
    await expect(section.locator('time')).toHaveAttribute('datetime', /\d{4}-\d{2}-\d{2}T/)
  } else {
    const alert = section.getByRole('alert')
    await expect(alert).toBeVisible()
    await expect(alert).toHaveAttribute('aria-live', 'polite')
    await expect(section.getByTestId('maintenance-repository-snapshot')).toHaveCount(0)
    await expect(section.getByRole('link', { name: 'Open connection guide' })).toBeVisible()
  }
  expect(methods.length).toBeGreaterThanOrEqual(1)
  expect(methods.every((method) => method === 'GET')).toBe(true)
})

test('keeps repository settings reachable and readable on a narrow screen', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  const { section } = await openRepositorySettings(page)
  await expect(
    section.getByRole('heading', { name: 'Maintenance repository', exact: true })
  ).toBeVisible()
  await expect(page.getByTestId('settings-mobile-nav')).toBeVisible()
  const select = page.getByTestId('settings-mobile-nav').getByRole('combobox')
  await select.selectOption('runtime')
  await expect(page).toHaveURL(/\/settings\/runtime$/)
  await select.selectOption('maintenance-repository')
  await expect(section).toBeVisible()
  await expect(page).toHaveURL(/\/settings\/maintenance-repository$/)
  expect(await section.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true)
  const guide = section.getByRole('link', { name: 'Open connection guide' })
  await guide.focus()
  await expect(guide).toBeFocused()
})
