import { render, screen } from '@testing-library/react'
import { describe, expect, test } from 'vitest'
import { ContextUsageDashboard } from '@app/features/analytics/ContextUsageDashboard'
import type { ContextUsageAnalytics } from '@app/shared/api/orchestration'

function analytics(overrides: Partial<ContextUsageAnalytics> = {}): ContextUsageAnalytics {
  return {
    lastRefreshedAt: '2026-05-20T12:00:00.000Z',
    staleAfterHours: 24,
    isStale: false,
    query: {
      limit: 8,
      minApplied: 1,
      staleAfterDays: 30,
      minSuccessRate: 0.5,
      negativeRate: 0.25,
    },
    summary: {
      rowCount: 0,
      distinctItems: 0,
      distinctAgents: 0,
      appliedCount: 0,
      completedCount: 0,
      successRate: 0,
      feedbackUsefulCount: 0,
      feedbackNegativeCount: 0,
    },
    topUseful: [],
    staleItems: [],
    needsReview: [],
    ...overrides,
  }
}

describe('ContextUsageDashboard', () => {
  test('explains empty reuse states without stale-threshold jargon', () => {
    render(<ContextUsageDashboard data={analytics()} />)

    expect(screen.getByText('Nothing looks outdated')).toBeDefined()
    expect(screen.getByText(/old enough to check again/i)).toBeDefined()
    expect(screen.getByText('Nothing needs review')).toBeDefined()
    expect(screen.queryByText(/stale threshold/i)).toBeNull()
    expect(screen.queryByText(/^Stale$/)).toBeNull()
    expect(screen.queryByText(/Snapshot/i)).toBeNull()
  })

  test('tells users to refresh old analytics before deciding from them', () => {
    render(<ContextUsageDashboard data={analytics({ isStale: true, staleAfterHours: 12 })} />)

    const banner = screen.getByTestId('context-usage-stale-banner')
    expect(banner).toHaveTextContent('These numbers are more than 12h old')
    expect(banner).toHaveTextContent('Refresh analytics before making decisions')
    expect(banner).not.toHaveTextContent('Snapshot')
  })
})
