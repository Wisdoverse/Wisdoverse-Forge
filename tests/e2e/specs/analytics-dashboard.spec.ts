import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

async function setupAnalyticsFixture(page: Page, baseURL: string) {
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

    const payload = btoa(
      JSON.stringify({ sub: 'user-1', exp: Math.floor(Date.now() / 1000) + 3600 })
    )
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/g, '')
    localStorage.setItem('af:auth:access', `e2e.${payload}.signature`)
    localStorage.setItem(
      'af:auth:user',
      JSON.stringify({ id: 'user-1', email: 'dev@example.com', name: 'Dev', role: 'admin' })
    )
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
  })

  await page.route('**/api/v1/analytics/context-usage**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        data: {
          lastRefreshedAt: '2026-05-05T08:00:00.000Z',
          lastRefreshStartedAt: '2026-05-05T08:00:00.000Z',
          lastRefreshError: null,
          staleAfterHours: 24,
          isStale: true,
          query: {
            limit: 8,
            minApplied: 10,
            staleAfterDays: 30,
            minSuccessRate: 0.7,
            negativeRate: 0.3,
          },
          summary: {
            rowCount: 3,
            distinctItems: 3,
            distinctAgents: 1,
            appliedCount: 42,
            completedCount: 35,
            successRate: 0.83,
            feedbackUsefulCount: 14,
            feedbackNegativeCount: 3,
          },
          topUseful: [
            {
              itemId: 'memory-top',
              itemKind: 'memory',
              itemTitle: 'Prod deploy memory',
              scopeKind: 'project',
              scopeId: 'proj-1',
              itemState: 'active',
              sensitivity: 'internal',
              lastVerifiedAt: '2026-05-04T08:00:00.000Z',
              taskKind: 'release',
              runtime: 'container',
              agentId: 'agent-1',
              agentName: 'Claude release agent',
              appliedCount: 20,
              completedCount: 18,
              successRate: 0.9,
              feedbackTotalCount: 8,
              feedbackUsefulCount: 7,
              feedbackNegativeCount: 0,
              negativeFeedbackRate: 0,
              lastUsedAt: '2026-05-05T07:00:00.000Z',
              lastFeedbackAt: '2026-05-05T07:10:00.000Z',
            },
          ],
          staleItems: [
            {
              itemId: 'memory-stale',
              itemKind: 'memory',
              itemTitle: 'Old deploy path',
              scopeKind: 'project',
              scopeId: 'proj-1',
              itemState: 'active',
              sensitivity: 'internal',
              lastVerifiedAt: '2026-03-01T08:00:00.000Z',
              taskKind: 'release',
              runtime: 'container',
              agentId: 'agent-1',
              agentName: 'Claude release agent',
              appliedCount: 1,
              completedCount: 1,
              successRate: 1,
              feedbackTotalCount: 0,
              feedbackUsefulCount: 0,
              feedbackNegativeCount: 0,
              negativeFeedbackRate: 0,
              lastUsedAt: '2026-03-01T08:00:00.000Z',
              lastFeedbackAt: null,
            },
          ],
          needsReview: [
            {
              itemId: 'skill-review',
              itemKind: 'skill',
              itemTitle: 'Release checklist',
              scopeKind: 'project',
              scopeId: 'proj-1',
              itemState: 'active',
              sensitivity: 'internal',
              lastVerifiedAt: '2026-05-01T08:00:00.000Z',
              taskKind: 'release',
              runtime: 'container',
              agentId: 'agent-1',
              agentName: 'Claude release agent',
              appliedCount: 12,
              completedCount: 9,
              successRate: 0.75,
              feedbackTotalCount: 6,
              feedbackUsefulCount: 2,
              feedbackNegativeCount: 4,
              negativeFeedbackRate: 0.67,
              lastUsedAt: '2026-05-05T07:00:00.000Z',
              lastFeedbackAt: '2026-05-05T07:20:00.000Z',
            },
          ],
        },
      }),
    })
  })
  await page.route('**/api/v1/analytics/summary**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        totalEvents: 100,
        toolCalls: 40,
        prompts: 30,
        responses: 30,
      }),
    })
  })
  await page.route('**/api/v1/analytics/tools**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, tools: [{ tool: 'Bash', count: 12, successRate: 0.9 }] }),
    })
  })
  await page.route('**/api/v1/analytics/activity**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        activity: [
          { hour: 9, count: 4 },
          { hour: 10, count: 8 },
        ],
      }),
    })
  })
  await page.route('**/api/v1/agents', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, agents: [{ id: 'agent-1', status: 'idle' }] }),
    })
  })
  await page.route('**/api/v1/context/features', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        data: { governance: true, preview: true, injection: true, analytics: true },
      }),
    })
  })

  await page.goto(`${baseURL}/analytics`)
  await page.locator('[data-testid="context-usage-dashboard"]').waitFor({ state: 'visible' })
}

test.describe('Analytics dashboard', () => {
  test('renders context usage analytics categories and staleness', async ({ page, baseURL }) => {
    await setupAnalyticsFixture(page, baseURL!)

    await expect(page.getByTestId('context-usage-stale-banner')).toBeVisible()
    await expect(page.getByTestId('context-usage-top-useful')).toContainText('Prod deploy memory')
    await expect(page.getByTestId('context-usage-needs-review')).toContainText('Release checklist')
    await expect(page.getByTestId('context-usage-stale-items')).toContainText('Old deploy path')
    await expect(page.getByTestId('context-usage-dashboard')).toContainText('83%')
  })
})
