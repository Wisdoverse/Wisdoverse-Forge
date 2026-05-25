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
})
