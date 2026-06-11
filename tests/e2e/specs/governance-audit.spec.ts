import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

const visibleMemoryId = '11111111-1111-4111-8111-111111111111'
const hiddenSkillHash = 'f9f0b5b53a25ad219cb741e8d15b3f2bb9a50f840b4f3300b814a7a2d18d2a66'

async function setupGovernanceAuditFixture(page: Page, baseURL: string) {
  await page.addInitScript(() => {
    class MockWebSocket {
      onopen: (() => void) | null = null
      onclose: (() => void) | null = null
      readyState = 1

      constructor() {
        window.setTimeout(() => this.onopen?.(), 0)
      }

      send() {}
      close() {
        this.readyState = 3
        this.onclose?.()
      }
    }

    Object.defineProperty(window, 'WebSocket', {
      value: MockWebSocket,
      configurable: true,
      writable: true,
    })

    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
  })

  await page.route('**/api/v1/governance/audit**', async (route) => {
    const url = new URL(route.request().url())
    const eventType = url.searchParams.get('eventType')
    const itemKind = url.searchParams.get('itemKind')
    const entries =
      eventType === 'governance.context.skill.reviewed' || itemKind === 'skill'
        ? [skillEntry()]
        : [visibleMemoryEntry(), hiddenSkillEntry()]

    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        data: {
          entries,
          query: {
            eventPrefix: 'governance.context.',
            limit: 50,
            offset: 0,
            redacted: true,
          },
        },
      }),
    })
  })

  await page.route('**/api/v1/governance/audit/export', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        data: {
          entries: [visibleMemoryEntry()],
          query: {
            eventPrefix: 'governance.context.',
            limit: 50,
            offset: 0,
            redacted: true,
          },
        },
      }),
    })
  })

  await page.goto(`${baseURL}/context/audit`, { waitUntil: 'domcontentloaded' })
  try {
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page.locator('[data-testid="governance-audit-view"]').waitFor({ state: 'visible' })
  } catch (err) {
    console.warn(`[governance-audit] app shell stalled on first nav; reloading once. ${err}`)
    await page.reload()
    await page.waitForLoadState('domcontentloaded')
    await page.locator('#root > *').first().waitFor({ state: 'attached', timeout: 30_000 })
    await page.locator('[data-testid="governance-audit-view"]').waitFor({ state: 'visible' })
  }
}

test.describe('Governance audit log', () => {
  test('renders scoped audit rows with hidden subjects and export', async ({ page, baseURL }) => {
    await setupGovernanceAuditFixture(page, baseURL!)

    await expect(page.getByTestId('governance-audit-row')).toHaveCount(2)
    await expect(page.getByTestId('governance-audit-item-reference')).toContainText('11111111')
    await expect(page.getByTestId('governance-audit-protected-reference')).toContainText(
      hiddenSkillHash.slice(0, 10)
    )
    await expect(page.getByTestId('governance-audit-redacted')).toBeVisible()
    await expect(page.getByText('Hidden item references')).toBeVisible()
    await expect(page.getByText('Support notes')).toBeVisible()
    await expect(page.getByText('Show support event').first()).toBeVisible()

    await page
      .getByTestId('governance-audit-filter-event-type')
      .fill('governance.context.skill.reviewed')
    await page.getByTestId('governance-audit-filter-item-kind').selectOption('skill')
    await page.getByRole('button', { name: 'Apply filters' }).click()

    await expect(page.getByTestId('governance-audit-row')).toHaveCount(1)
    await expect(page.getByTestId('governance-audit-view')).toContainText('Skill reviewed')

    const download = page.waitForEvent('download')
    await page.getByTestId('governance-audit-export').click()
    await download
    await expect(page.getByText('1 audit event exported')).toBeVisible()
  })
})

function visibleMemoryEntry() {
  return {
    id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
    eventType: 'governance.context.feedback.recorded',
    actorUserId: 'user-1',
    itemKind: 'memory',
    scopeKind: 'project',
    scopeId: 'proj-1',
    rawItemId: visibleMemoryId,
    auditSubjectHash: 'ad7757d90838970a9fd7ec617f8f8f278621b2255c36461870672d4d015bcfb7',
    resourceType: 'memory_item',
    resourceId: visibleMemoryId,
    details: { item_kind: 'memory', label: 'useful' },
    detailsRedacted: false,
    tamperStatus: 'not_configured',
    createdAt: '2026-05-05T08:00:00.000Z',
  }
}

function hiddenSkillEntry() {
  return {
    id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
    eventType: 'governance.context.skill.approved',
    actorUserId: 'user-2',
    itemKind: 'skill',
    scopeKind: 'project',
    scopeId: 'proj-hidden',
    rawItemId: null,
    auditSubjectHash: hiddenSkillHash,
    resourceType: 'skill',
    resourceId: '22222222-2222-4222-8222-222222222222',
    details: { api_key: '[REDACTED]', item_kind: 'skill' },
    detailsRedacted: true,
    tamperStatus: 'valid',
    createdAt: '2026-05-05T09:00:00.000Z',
  }
}

function skillEntry() {
  return {
    ...hiddenSkillEntry(),
    id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
    eventType: 'governance.context.skill.reviewed',
  }
}
