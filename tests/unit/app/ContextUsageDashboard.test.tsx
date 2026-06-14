import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test } from 'vitest'
import { ContextUsageDashboard } from '@app/features/analytics/ContextUsageDashboard'
import type { ContextUsageAnalytics } from '@app/shared/api/orchestration'

afterEach(cleanup)

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
    const previousSavedNotesCopy = new RegExp(['saved', 'memories'].join('\\s+'), 'i')

    render(<ContextUsageDashboard data={analytics()} />)

    expect(screen.getByText('Nothing looks outdated')).toBeDefined()
    expect(
      screen.getByText(/saved notes and saved instructions appear here when they are old enough/i)
    ).toBeDefined()
    expect(screen.getByText('Mark useful saved items to rank them here')).toBeDefined()
    expect(screen.getByText(/choose Useful in the task result/i)).toBeDefined()
    expect(screen.getByText(/old enough to check again/i)).toBeDefined()
    expect(screen.getByText('Nothing to check right now')).toBeDefined()
    expect(screen.getByText(/people report they may be outdated/i)).toBeDefined()
    expect(screen.getByText('Check first')).toBeDefined()
    expect(screen.getByText('Items people marked for another look.')).toBeDefined()
    expect(screen.queryByText(previousSavedNotesCopy)).toBeNull()
    expect(screen.queryByText('No useful saved items yet')).toBeNull()
    expect(screen.queryByText(/stale threshold/i)).toBeNull()
    expect(screen.queryByText(/^Stale$/)).toBeNull()
    expect(screen.queryByText(/Snapshot/i)).toBeNull()
    expect(screen.queryByText('Needs Review')).toBeNull()
    expect(screen.queryByText('Signals to check before reuse.')).toBeNull()
  })

  test('tells users to refresh old analytics before deciding from them', () => {
    render(<ContextUsageDashboard data={analytics({ isStale: true, staleAfterHours: 12 })} />)

    const banner = screen.getByTestId('context-usage-stale-banner')
    expect(banner).toHaveTextContent('These numbers are more than 12h old')
    expect(banner).toHaveTextContent('Refresh analytics before making decisions')
    expect(banner).not.toHaveTextContent('Snapshot')
  })

  test('uses clear fallback text when analytics labels are missing', () => {
    render(
      <ContextUsageDashboard
        data={analytics({
          lastRefreshedAt: 'not-a-date',
          topUseful: [
            {
              itemId: 'memory-1',
              itemKind: 'memory',
              itemTitle: 'Release checklist',
              taskKind: '',
              runtime: '',
              agentId: 'agent-1',
              agentName: 'Builder Agent',
              appliedCount: 2,
              completedCount: 2,
              successRate: 1,
              feedbackTotalCount: 1,
              feedbackUsefulCount: 1,
              feedbackNegativeCount: 0,
              negativeFeedbackRate: 0,
              lastUsedAt: '2026-05-20T12:00:00.000Z',
            },
          ],
        })}
      />
    )

    expect(screen.getByText('Updated time not available')).toBeDefined()
    const item = screen.getByTestId('context-usage-item')
    expect(item.textContent).toContain(
      'Builder Agent · Work location not listed · Task type not listed'
    )
    expect(item.textContent).toContain('Next: keep this available for similar tasks.')
    expect(screen.queryByText(/^unknown$/i)).toBeNull()
  })

  test('labels unknown analytics group values without exposing raw slugs', () => {
    render(
      <ContextUsageDashboard
        data={analytics({
          topUseful: [
            {
              itemId: 'context-1',
              itemKind: 'future_context_kind' as never,
              itemTitle: 'Release checklist',
              taskKind: 'future_task_kind',
              runtime: 'future_runtime',
              agentId: 'agent-1',
              agentName: 'Builder Agent',
              appliedCount: 2,
              completedCount: 2,
              successRate: 1,
              feedbackTotalCount: 1,
              feedbackUsefulCount: 1,
              feedbackNegativeCount: 0,
              negativeFeedbackRate: 0,
              lastUsedAt: '2026-05-20T12:00:00.000Z',
            },
          ],
        })}
      />
    )

    const item = screen.getByTestId('context-usage-item')
    expect(item.textContent).toContain('Saved item')
    expect(item.textContent).not.toContain(['Con', 'text item needs review'].join(''))
    expect(item.textContent).toContain(
      'Builder Agent · Work location needs review · Task type needs review'
    )
    expect(screen.queryByText(/future context kind/i)).toBeNull()
    expect(screen.queryByText(/future runtime/i)).toBeNull()
    expect(screen.queryByText(/future task kind/i)).toBeNull()
  })

  test('adds a plain next step for context that needs review', () => {
    render(
      <ContextUsageDashboard
        data={analytics({
          needsReview: [
            {
              itemId: 'memory-1',
              itemKind: 'memory',
              itemTitle: 'Old release note',
              taskKind: 'review',
              runtime: 'cli',
              agentId: 'agent-1',
              agentName: 'Reviewer Agent',
              appliedCount: 3,
              completedCount: 1,
              successRate: 0.33,
              feedbackTotalCount: 2,
              feedbackUsefulCount: 0,
              feedbackNegativeCount: 2,
              negativeFeedbackRate: 1,
              lastUsedAt: '2026-05-20T12:00:00.000Z',
            },
          ],
        })}
      />
    )

    const item = screen.getByTestId('context-usage-item')
    expect(item.textContent).toContain(
      'Next: open the latest task result, then update or remove this before reuse.'
    )
  })
})
