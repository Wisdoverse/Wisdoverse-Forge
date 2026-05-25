import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest'
import { render, screen, fireEvent, cleanup } from '@testing-library/react'
import { AnalyticsDashboard } from '@app/features/analytics/AnalyticsDashboard'
import { useAnalyticsStore } from '@app/shared/model/analytics.store'

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
    loading: false,
    error: null,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
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
    expect(screen.getByText('No activity data')).toBeDefined()
  })

  test('points beginners at the busiest low-success tool first', () => {
    useAnalyticsStore.setState({
      tools: [{ tool: 'Bash', count: 12, successRate: 0.42 }],
      agentStats: { total: 2, online: 2, offline: 0, working: 0 },
    })

    render(<AnalyticsDashboard />)

    const nextStep = screen.getByTestId('analytics-next-step')
    expect(nextStep).toHaveTextContent('Review Bash failures first')
    expect(nextStep).toHaveTextContent('completed cleanly only 42%')
    expect(screen.getByText('Busiest tool')).toBeDefined()
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
    // Last bar: 13:00, 8 events
    expect(detail.textContent).toContain('13:00')
    expect(detail.textContent).toContain('8 events')
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
    expect(detail.textContent).toContain('20 events')
    expect(screen.getByText(/47% of window/)).toBeDefined()
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
