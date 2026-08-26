import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ContextSafetyInsight } from '@app/features/analytics/ContextSafetyInsight'

const listAnalyticsEventsMock = vi.hoisted(() => vi.fn().mockResolvedValue([]))

vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      listAnalyticsEvents: listAnalyticsEventsMock,
    },
  }
})

afterEach(cleanup)

describe('ContextSafetyInsight', () => {
  test('shows the empty hint before any context signals exist', async () => {
    listAnalyticsEventsMock.mockResolvedValue([])

    render(<ContextSafetyInsight />)

    expect(await screen.findByText(/Signals appear after a task is prepared/i)).toBeInTheDocument()
  })

  test('counts warnings and failures and links a prior warning to the failure', async () => {
    listAnalyticsEventsMock.mockImplementation((name: string) => {
      if (name === 'context_budget_warning') {
        return Promise.resolve([
          { event_name: name, properties: { taskId: 't1' }, created_at: '2026-05-06T08:00:00Z' },
          { event_name: name, properties: { taskId: 't2' }, created_at: '2026-05-06T08:30:00Z' },
        ])
      }
      if (name === 'context_trim_applied') {
        return Promise.resolve([])
      }
      return Promise.resolve([
        { event_name: name, properties: { taskId: 't1' }, created_at: '2026-05-06T09:00:00Z' },
        { event_name: name, properties: { taskId: 't3' }, created_at: '2026-05-06T09:10:00Z' },
        { event_name: name, properties: { taskId: 't2' }, created_at: '2026-05-06T08:20:00Z' },
      ])
    })

    render(<ContextSafetyInsight />)

    expect(await screen.findByText(/2 context warnings shown before task runs/)).toBeInTheDocument()
    expect(screen.getByText(/3 tasks ran out of context window/)).toBeInTheDocument()
    expect(screen.getByText(/33% of those failures had a context warning beforehand/)).toBeInTheDocument()
    expect(screen.getByText(/the warning did not keep the agent inside its budget/)).toBeInTheDocument()
    expect(screen.getByText(/2 context warnings shown before task runs/)).toBeInTheDocument()
  })

  test('counts trims applied after warnings', async () => {
    listAnalyticsEventsMock.mockImplementation((name: string) => {
      if (name === 'context_trim_applied') {
        return Promise.resolve([
          { event_name: name, properties: { taskId: 't1' }, created_at: '2026-05-06T08:10:00Z' },
          { event_name: name, properties: { taskId: 't1' }, created_at: '2026-05-06T08:15:00Z' },
          { event_name: name, properties: { taskId: 't2' }, created_at: '2026-05-06T08:40:00Z' },
        ])
      }
      return Promise.resolve([])
    })

    render(<ContextSafetyInsight />)

    expect(await screen.findByText(/2 trims applied/)).toBeInTheDocument()
  })
})
