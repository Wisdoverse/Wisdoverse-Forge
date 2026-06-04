import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ContextUsageDashboard } from '@app/features/analytics/ContextUsageDashboard'
import type { ContextUsageAnalytics } from '@app/shared/api/orchestration'

const baseData: ContextUsageAnalytics = {
  lastRefreshedAt: '2026-05-25T11:45:00Z',
  lastRefreshStartedAt: null,
  lastRefreshError: null,
  staleAfterHours: 12,
  isStale: false,
  query: {
    limit: 10,
    minApplied: 2,
    staleAfterDays: 14,
    minSuccessRate: 0.8,
    negativeRate: 0.2,
  },
  summary: {
    rowCount: 0,
    distinctItems: 0,
    distinctAgents: 0,
    appliedCount: 8,
    completedCount: 6,
    successRate: 0.75,
    feedbackUsefulCount: 3,
    feedbackNegativeCount: 1,
  },
  topUseful: [],
  staleItems: [],
  needsReview: [],
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('ContextUsageDashboard', () => {
  test('explains context reuse metrics for beginner operators', () => {
    vi.spyOn(Date, 'now').mockReturnValue(new Date('2026-05-25T12:00:00Z').getTime())

    render(<ContextUsageDashboard data={baseData} />)

    expect(
      screen.getByText(
        'Use this panel to keep context that helps work finish and review items that may be outdated, incorrect, or too sensitive before agents reuse them.'
      )
    ).toBeDefined()
    expect(screen.getByText('Times context was added to agent work.')).toBeDefined()
    expect(screen.getByText('Completed work after context was used.')).toBeDefined()
    expect(screen.getByText('Times users marked the context helpful.')).toBeDefined()
    expect(screen.getByText('Signals to check before reuse.')).toBeDefined()
    expect(screen.getByText('Snapshot refreshed 15m ago')).toBeDefined()
  })

  test('turns empty lists into next-step guidance', () => {
    render(<ContextUsageDashboard data={baseData} />)

    expect(
      screen.getByText('Helpful items appear after users mark applied context as useful.')
    ).toBeDefined()
    expect(
      screen.getByText(
        'Items show here when feedback says context may be outdated, incorrect, or sensitive.'
      )
    ).toBeDefined()
    expect(
      screen.getByText('Nothing has crossed the stale threshold for this workspace.')
    ).toBeDefined()
  })

  test('makes stale snapshots actionable', () => {
    render(<ContextUsageDashboard data={{ ...baseData, isStale: true }} />)

    expect(screen.getByTestId('context-usage-stale-banner').textContent).toContain(
      'Refresh analytics before acting on these numbers.'
    )
  })

  test('turns machine item labels into beginner-readable copy', () => {
    render(
      <ContextUsageDashboard
        data={{
          ...baseData,
          topUseful: [
            {
              itemId: 'b6f4e2f4-5e28-4b36-bced-4d0e1bb32c3d',
              itemKind: 'memory',
              itemTitle: 'Release checklist',
              scopeKind: 'workspace',
              scopeId: '22c49f3c-6485-4d47-a263-343e0449b496',
              itemState: 'active',
              sensitivity: 'internal',
              lastVerifiedAt: null,
              taskKind: 'coding',
              runtime: 'container',
              agentId: 'a2d0b101-b64e-4b0e-a93a-0a9d02f1c55c',
              agentName: 'Planner Agent',
              appliedCount: 4,
              completedCount: 3,
              successRate: 0.75,
              feedbackTotalCount: 2,
              feedbackUsefulCount: 1,
              feedbackNegativeCount: 1,
              negativeFeedbackRate: 0.5,
              lastUsedAt: '2026-05-25T11:45:00Z',
              lastFeedbackAt: '2026-05-25T11:50:00Z',
            },
          ],
        }}
      />
    )

    expect(screen.getByText('Saved memory')).toBeDefined()
    expect(screen.getByText('Planner Agent · Managed workspace · Code change')).toBeDefined()
    expect(screen.getByText('review')).toBeDefined()
    expect(screen.queryByText('memory')).toBeNull()
    expect(screen.queryByText('Planner Agent · container · coding')).toBeNull()
    expect(screen.queryByText('negative')).toBeNull()
  })
})
