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
  test('explains saved item reuse metrics for beginner operators', () => {
    vi.spyOn(Date, 'now').mockReturnValue(new Date('2026-05-25T12:00:00Z').getTime())

    render(<ContextUsageDashboard data={baseData} />)

    expect(screen.getByText('Saved item reuse')).toBeDefined()
    expect(
      screen.getByText(
        'Use this panel to keep saved notes and instructions that help work finish, and review items that may be outdated, incorrect, or too sensitive before agents reuse them.'
      )
    ).toBeDefined()
    expect(
      screen.getByText('Times saved notes or instructions were added to agent work.')
    ).toBeDefined()
    expect(screen.getByText('Completed work after saved items were used.')).toBeDefined()
    expect(screen.getByText('Times users marked saved items helpful.')).toBeDefined()
    expect(screen.getByText('Signals to check before reuse.')).toBeDefined()
    expect(screen.getByText('Updated 15m ago')).toBeDefined()
  })

  test('turns empty lists into next-step guidance', () => {
    render(<ContextUsageDashboard data={baseData} />)

    expect(
      screen.getByText(
        'Helpful saved notes and saved instructions appear after people mark them useful in task results.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        'Items appear here when feedback says they may be outdated, incorrect, or too sensitive.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        'Saved notes and saved instructions appear here when they are old enough to check again.'
      )
    ).toBeDefined()
  })

  test('makes stale snapshots actionable', () => {
    render(<ContextUsageDashboard data={{ ...baseData, isStale: true }} />)

    expect(screen.getByTestId('context-usage-stale-banner').textContent).toContain(
      'Refresh analytics before making decisions from them.'
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

    expect(screen.getByText('Saved note')).toBeDefined()
    expect(screen.getByText('Planner Agent · Managed workspace · Code change')).toBeDefined()
    expect(screen.getByText('Next: keep this available for similar tasks.')).toBeDefined()
    expect(screen.getByText('review')).toBeDefined()
    expect(screen.queryByText(/Saved\s+memory/)).toBeNull()
    expect(screen.queryByText('memory')).toBeNull()
    expect(screen.queryByText('Planner Agent · container · coding')).toBeNull()
    expect(screen.queryByText('negative')).toBeNull()
  })
})
