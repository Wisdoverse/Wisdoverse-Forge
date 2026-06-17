import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup } from '@testing-library/react'
import { AnalyticsDashboard } from '@app/features/analytics/AnalyticsDashboard'
import { useAnalyticsStore } from '@app/shared/model/analytics.store'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

// The dashboard kicks off a `load()` on mount. Stub it so we render
// synchronously with a curated slice of state.
beforeEach(() => {
  vi.spyOn(useAnalyticsStore.getState(), 'load').mockResolvedValue()
  useAnalyticsStore.setState({
    dateRange: 'today',
    summary: { totalEvents: 100, toolCalls: 40, prompts: 30, responses: 30 },
    tools: [],
    hourly: [
      { hour: 10, count: 5 },
      { hour: 11, count: 10 },
      { hour: 12, count: 20 },
      { hour: 13, count: 8 },
    ],
    agentStats: { total: 3, online: 2, offline: 1, working: 1 },
    contextUsage: null,
    loading: false,
    error: null,
  })
  useContextFeaturesStore.getState().reset()
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useContextFeaturesStore.getState().reset()
})

describe('AnalyticsDashboard · ActivityBarChart', () => {
  test('explains the next operator action before the raw metrics', () => {
    render(<AnalyticsDashboard />)

    const nextStep = screen.getByTestId('analytics-next-step')
    expect(nextStep).toHaveTextContent('What to check next')
    expect(nextStep).toHaveTextContent('Bring offline agents back before judging work')
    expect(nextStep).toHaveTextContent('Open Agents and reconnect or restart the offline agents')
  })

  test('guides an empty activity range toward running a first task', () => {
    useAnalyticsStore.setState({
      summary: { totalEvents: 0, toolCalls: 0, prompts: 0, responses: 0 },
      tools: [],
      hourly: [],
      agentStats: { total: 1, online: 1, offline: 0, working: 0 },
    })

    render(<AnalyticsDashboard />)

    const nextStep = screen.getByTestId('analytics-next-step')
    expect(nextStep).toHaveTextContent('Start a task to create activity data')
    expect(nextStep).toHaveTextContent('Create one simple task')
    expect(screen.getByText('Run a task to fill this chart')).toBeDefined()
    expect(screen.getByText('Tool use appears after an agent finishes a task')).toBeDefined()
    expect(screen.queryByText('Tool use appears after an agent runs a task')).toBeNull()
    expect(screen.queryByText('No activity data')).toBeNull()
    expect(screen.queryByText('No tool usage data')).toBeNull()
  })

  test('explains how analytics starts when no agents exist yet', () => {
    useAnalyticsStore.setState({
      summary: { totalEvents: 0, toolCalls: 0, prompts: 0, responses: 0 },
      tools: [],
      hourly: [],
      agentStats: { total: 0, online: 0, offline: 0, working: 0 },
    })

    render(<AnalyticsDashboard />)

    const nextStep = screen.getByTestId('analytics-next-step')
    expect(nextStep).toHaveTextContent('Create or connect an agent first')
    expect(nextStep).toHaveTextContent(
      'This page starts showing trends after at least one agent is connected and has run a task.'
    )
    expect(nextStep).toHaveTextContent('Open Agents, add one agent')
    expect(nextStep).not.toHaveTextContent('No agents are reporting status yet')
  })

  test('points beginners at the busiest low-success tool first', () => {
    useAnalyticsStore.setState({
      tools: [{ tool: 'shell_command', count: 12, successRate: 0.42 }],
      agentStats: { total: 2, online: 2, offline: 0, working: 0 },
    })

    render(<AnalyticsDashboard />)

    const nextStep = screen.getByTestId('analytics-next-step')
    expect(nextStep).toHaveTextContent('Review Command line recovery first')
    expect(nextStep).toHaveTextContent('completed cleanly only 42%')
    expect(nextStep).toHaveTextContent('review the recovery notes')
    expect(nextStep).not.toHaveTextContent('failed tool steps')
    expect(nextStep).not.toHaveTextContent('failures first')
    expect(nextStep).not.toHaveTextContent('ended in error')
    expect(screen.getByText('Busiest tool')).toBeDefined()
    expect(screen.getAllByText('Command line').length).toBeGreaterThan(0)
    expect(screen.queryByText('shell_command')).toBeNull()
  })

  test('shows a retry action when analytics cannot load', () => {
    const load = vi.fn().mockResolvedValue(undefined)
    useAnalyticsStore.setState({
      load,
      error: 'Check your connection, then refresh the dashboard. Analytics could not connect.',
    })

    render(<AnalyticsDashboard />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Refresh analytics data')
    expect(alert).toHaveTextContent('Check your connection, then refresh the dashboard.')
    fireEvent.click(screen.getByRole('button', { name: /refresh dashboard/i }))
    expect(load).toHaveBeenCalled()
  })

  test('shows range refresh progress and locks range controls', () => {
    const setDateRange = vi.fn()
    useAnalyticsStore.setState({
      dateRange: '7d',
      loading: true,
      setDateRange,
    })

    render(<AnalyticsDashboard />)

    expect(screen.getByText('Refreshing Last 7 days...')).toBeDefined()
    const currentRange = screen.getByRole('button', { name: /last 7 days, refreshing now/i })
    expect(currentRange).toBeDisabled()
    expect(currentRange).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: 'Today' })).toBeDisabled()

    fireEvent.click(screen.getByRole('button', { name: 'Today' }))
    expect(setDateRange).not.toHaveBeenCalled()
  })

  test('labels local agent work without container jargon', () => {
    useContextFeaturesStore.setState({ analytics: true, loaded: true, loading: false })
    useAnalyticsStore.setState({
      contextUsage: {
        lastRefreshedAt: new Date().toISOString(),
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
          rowCount: 1,
          distinctItems: 1,
          distinctAgents: 1,
          appliedCount: 3,
          completedCount: 3,
          successRate: 1,
          feedbackUsefulCount: 1,
          feedbackNegativeCount: 0,
        },
        topUseful: [
          {
            itemId: 'memory-1',
            itemKind: 'memory',
            itemTitle: 'Release checklist',
            taskKind: 'coding',
            runtime: 'cli',
            agentId: 'agent-1',
            agentName: 'Local Agent',
            appliedCount: 3,
            completedCount: 3,
            successRate: 1,
            feedbackTotalCount: 1,
            feedbackUsefulCount: 1,
            feedbackNegativeCount: 0,
            negativeFeedbackRate: 0,
            lastUsedAt: new Date().toISOString(),
          },
        ],
        staleItems: [],
        needsReview: [],
      },
    })

    render(<AnalyticsDashboard />)

    const item = screen.getByTestId('context-usage-item')
    expect(item.textContent).toContain('Local Agent · This computer · Code change')
    expect(item.textContent).not.toContain('Container CLI')
  })

  test('renders the hourly activity chart with axis labels', () => {
    render(<AnalyticsDashboard />)
    const chart = screen.getByTestId('activity-chart')
    expect(chart).toBeDefined()
    // First and last bar labels should be visible in the axis
    expect(chart.textContent).toContain('10:00')
    expect(chart.textContent).toContain('13:00')
  })

  test('shows the most-recent bar detail by default', () => {
    render(<AnalyticsDashboard />)
    const detail = screen.getByTestId('activity-chart-detail')
    // Last bar: 13:00, 8 updates
    expect(detail.textContent).toContain('13:00')
    expect(detail.textContent).toContain('8 updates')
    expect(screen.getByRole('group', { name: /hourly work updates/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /13:00: 8 updates/i })).toBeDefined()
    expect(detail).not.toHaveTextContent(/events/i)
    expect(screen.queryByRole('group', { name: /hourly event activity/i })).toBeNull()
    expect(screen.getByText('most recent')).toBeDefined()
  })

  test('updates the detail header on bar hover with share-of-window', () => {
    render(<AnalyticsDashboard />)
    // Hover the 12:00 bar (third bar, index 2, count=20 out of 43 total ≈ 47%)
    const bars = screen.getByTestId('activity-chart').querySelectorAll('button')
    expect(bars.length).toBe(4)
    fireEvent.mouseEnter(bars[2])

    const detail = screen.getByTestId('activity-chart-detail')
    expect(detail.textContent).toContain('12:00')
    expect(detail.textContent).toContain('20 updates')
    expect(screen.getByText(/47% of shown hours/)).toBeDefined()
    expect(screen.queryByText(/47% of window/)).toBeNull()
  })

  test('restores most-recent label when mouse leaves the chart', () => {
    render(<AnalyticsDashboard />)
    const chart = screen.getByTestId('activity-chart')
    const bars = chart.querySelectorAll('button')
    fireEvent.mouseEnter(bars[0])
    expect(screen.getByTestId('activity-chart-detail').textContent).toContain('10:00')
    fireEvent.mouseLeave(chart)
    expect(screen.getByText('most recent')).toBeDefined()
  })
})
