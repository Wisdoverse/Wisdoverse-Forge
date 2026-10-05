import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

const MOBILE_VIEWPORT = { width: 320, height: 740 }
const DESKTOP_VIEWPORT = { width: 1440, height: 900 }

async function seedWorkspacePreferences(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
    localStorage.setItem('af:nav:sidebarExpanded', 'true')
  })
}

async function openTaskBoard(
  page: Page,
  baseURL: string,
  viewport = DESKTOP_VIEWPORT
): Promise<void> {
  await page.setViewportSize(viewport)
  await seedWorkspacePreferences(page)
  await page.goto(`${baseURL}/tasks`)
  await expect(page.getByTestId('page-tasks')).toBeVisible()
  await expect(page.getByTestId('board-toolbar')).toBeVisible()
}

async function hasNoHorizontalOverflow(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    const root = document.documentElement
    return root.scrollWidth <= root.clientWidth + 2
  })
}

function contrastRatio(foreground: string, background: string): number {
  function luminance(color: string): number {
    const channels = color
      .match(/[\d.]+/g)
      ?.slice(0, 3)
      .map(Number)
    if (!channels || channels.length !== 3) throw new Error(`Unexpected computed color: ${color}`)
    const linear = channels.map((channel) => {
      const value = channel / 255
      return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
    })
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
  }

  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a)
  return (values[0] + 0.05) / (values[1] + 0.05)
}

async function topBarSecondaryContrast(page: Page): Promise<number> {
  const subtitle = page.getByText('Create tasks and follow agent progress', { exact: true })
  await expect(subtitle).toBeVisible()
  const colors = await subtitle.evaluate((element) => {
    const topBar = element.closest('[data-testid="top-bar"]')
    if (!topBar) throw new Error('Top bar was not found')
    return {
      foreground: getComputedStyle(element).color,
      background: getComputedStyle(topBar).backgroundColor,
    }
  })
  return contrastRatio(colors.foreground, colors.background)
}

test.describe('UI modernization', () => {
  test('uses the saved Chinese language for translated controls and assistive technology', async ({
    page,
    baseURL,
  }) => {
    await page.addInitScript(() => localStorage.setItem('af:lang', 'zh'))
    await openTaskBoard(page, baseURL!, MOBILE_VIEWPORT)

    await expect(page.getByRole('combobox', { name: '任务视图' })).toBeVisible()
    await expect(page.getByText('智能体与任务详情', { exact: true })).toBeVisible()
    await expect(page.locator('html')).toHaveAttribute('lang', 'zh')
    expect(await hasNoHorizontalOverflow(page)).toBe(true)
  })

  test('switches task views at 320 pixels and keeps controls usable', async ({ page, baseURL }) => {
    await openTaskBoard(page, baseURL!, MOBILE_VIEWPORT)

    const selector = page.getByRole('combobox', { name: 'Task view' })
    const search = page.getByTestId('top-bar-command-search')
    const createTask = page.getByRole('button', { name: 'New task', exact: true })

    await expect(selector).toBeVisible()
    await selector.selectOption('list')
    await expect(page.getByTestId('list-work-register')).toBeVisible()
    await selector.selectOption('board')
    await expect(page.getByTestId('board-toolbar')).toBeVisible()

    for (const control of [selector, search, createTask]) {
      await expect(control).toBeVisible()
      const box = await control.boundingBox()
      expect(box).not.toBeNull()
      expect(box!.width).toBeGreaterThanOrEqual(44)
      expect(box!.height).toBeGreaterThanOrEqual(44)
    }
    expect(await hasNoHorizontalOverflow(page)).toBe(true)
  })

  test('keeps task search usable when the activity panel narrows the workspace', async ({
    page,
    baseURL,
  }) => {
    await openTaskBoard(page, baseURL!, { width: 1040, height: 768 })
    const panelToggle = page.getByTestId('activity-panel-toggle')
    if (await panelToggle.isVisible()) await panelToggle.click()

    const search = page.getByTestId('board-search')
    await expect(search).toBeVisible()
    const searchBox = await search.boundingBox()
    expect(searchBox).not.toBeNull()
    expect(searchBox!.width).toBeGreaterThanOrEqual(200)
    const help = page.getByText(/Search only narrows tasks shown below\./)
    const helpBox = await help.boundingBox()
    expect(helpBox).not.toBeNull()
    expect(helpBox!.height).toBeLessThan(80)
    expect(await hasNoHorizontalOverflow(page)).toBe(true)
  })

  test('traps mobile navigation focus and closes on Escape or navigation', async ({
    page,
    baseURL,
  }) => {
    await openTaskBoard(page, baseURL!, { ...MOBILE_VIEWPORT, height: 568 })
    const trigger = page.getByRole('button', { name: 'Open navigation' })

    await trigger.click()
    const dialog = page.getByRole('dialog', { name: 'Workspace navigation' })
    await expect(dialog).toBeVisible()
    await expect
      .poll(() =>
        page.evaluate(() => {
          const navigation = document.querySelector('dialog[aria-label="Workspace navigation"]')
          return Boolean(navigation?.contains(document.activeElement))
        })
      )
      .toBe(true)

    await trigger.evaluate((element) => element.focus())
    expect(await dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true)
    await page.keyboard.press('Shift+Tab')
    expect(
      await page.evaluate(() => {
        const navigation = document.querySelector('dialog[aria-label="Workspace navigation"]')
        // Native dialogs allow focus to reach browser chrome. App content stays inert.
        return (
          document.activeElement === document.body ||
          Boolean(navigation?.contains(document.activeElement))
        )
      })
    ).toBe(true)
    await page.keyboard.press('Tab')
    expect(await dialog.evaluate((element) => element.contains(document.activeElement))).toBe(true)
    const project = dialog.getByTestId('project-proj-1')
    await project.scrollIntoViewIfNeeded()
    const projectBox = await project.boundingBox()
    const dialogBox = await dialog.boundingBox()
    expect(projectBox).not.toBeNull()
    expect(dialogBox).not.toBeNull()
    expect(projectBox!.height).toBeGreaterThanOrEqual(44)
    expect(projectBox!.y).toBeGreaterThanOrEqual(dialogBox!.y)
    expect(projectBox!.y + projectBox!.height).toBeLessThanOrEqual(dialogBox!.y + dialogBox!.height)
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
    await expect(trigger).toBeFocused()

    await trigger.click()
    await dialog.getByTestId('sidebar-nav-inbox').click()
    await page.waitForURL('**/inbox')
    await expect(dialog).toBeHidden()
    await expect(page.getByRole('heading', { name: 'Inbox', exact: true })).toBeVisible()
  })

  test('keeps palette shortcuts inside its native dialog and returns focus', async ({
    page,
    baseURL,
  }) => {
    await openTaskBoard(page, baseURL!)
    const trigger = page.getByTestId('top-bar-command-search')
    await trigger.click()

    const palette = page.getByRole('dialog', { name: 'Find what you need' })
    const searchInput = palette.getByRole('combobox', { name: 'Search pages and things to do' })
    await expect(palette).toBeVisible()
    await expect(searchInput).toBeFocused()

    await page.keyboard.press('n')
    await page.keyboard.down('Control')
    await page.keyboard.press('\\')
    await page.keyboard.up('Control')
    await expect(palette).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Tell an agent what to do' })).toHaveCount(0)
    await expect(page.getByRole('dialog', { name: 'Workspace navigation' })).toHaveCount(0)

    await page.keyboard.press('Escape')
    await expect(palette).toBeHidden()
    await expect(trigger).toBeFocused()
  })

  test('opens the ready task form and returns focus without sending a task', async ({
    page,
    baseURL,
  }) => {
    const taskWrites: string[] = []
    page.on('request', (request) => {
      if (
        request.method() === 'POST' &&
        /\/api\/v1\/orchestration\/groups\/[^/]+\/tasks(?:\?|$)/.test(request.url())
      ) {
        taskWrites.push(request.url())
      }
    })
    await openTaskBoard(page, baseURL!)
    const trigger = page.getByRole('button', { name: 'New task', exact: true })
    await expect(trigger).toBeVisible()
    await trigger.click()

    const dialog = page.getByRole('dialog', { name: 'Tell an agent what to do' })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByLabel('What should the agent finish?')).toBeFocused()
    await page.keyboard.press('Escape')
    await expect(dialog).toBeHidden()
    await expect(trigger).toBeFocused()
    expect(taskWrites).toEqual([])
  })

  test('keeps secondary text readable and requests local fonts only', async ({ page, baseURL }) => {
    const requestedFonts: string[] = []
    const remoteFontRequests: string[] = []
    page.on('request', (request) => {
      if (request.resourceType() !== 'font') return
      requestedFonts.push(request.url())
      if (/fonts\.(?:googleapis|gstatic)\.com/i.test(request.url())) {
        remoteFontRequests.push(request.url())
      }
    })
    await openTaskBoard(page, baseURL!)
    await page.evaluate(() => document.fonts.ready.then(() => true))

    const themeToggle = page.getByRole('button', { name: /Switch to (light|dark) mode/ })
    if ((await themeToggle.getAttribute('aria-label')) === 'Switch to light mode') {
      await themeToggle.click()
    }
    expect(await topBarSecondaryContrast(page)).toBeGreaterThanOrEqual(4.5)

    await page.getByRole('button', { name: 'Switch to dark mode' }).click()
    await expect(page.getByRole('button', { name: 'Switch to light mode' })).toBeVisible()
    expect(await topBarSecondaryContrast(page)).toBeGreaterThanOrEqual(4.5)

    expect(requestedFonts.length).toBeGreaterThan(0)
    expect(requestedFonts.every((url) => new URL(url).origin === new URL(baseURL!).origin)).toBe(
      true
    )
    expect(remoteFontRequests).toEqual([])
  })
})
