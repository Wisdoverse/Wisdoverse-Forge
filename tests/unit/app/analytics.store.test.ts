import { beforeEach, describe, expect, test, vi } from 'vitest'

const agentApiMock = vi.hoisted(() => ({
  getAnalyticsSummary: vi.fn(),
  getAnalyticsTools: vi.fn(),
  getAnalyticsActivity: vi.fn(),
  getAgents: vi.fn(),
}))

const getAgentApiMock = vi.hoisted(() => vi.fn(() => agentApiMock))
const fetchContextUsageAnalyticsMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/legacy', () => ({
  getAgentApi: getAgentApiMock,
}))

vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: {
    fetchContextUsageAnalytics: fetchContextUsageAnalyticsMock,
  },
}))

import {
  analyticsNetworkErrorMessage,
  analyticsUnavailableMessage,
  useAnalyticsStore,
} from '@app/shared/model/analytics.store'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

function resetAnalyticsStore() {
  useAnalyticsStore.setState({
    dateRange: 'today',
    summary: null,
    tools: [],
    hourly: [],
    agentStats: { total: 0, online: 0, offline: 0, working: 0 },
    contextUsage: null,
    loading: false,
    error: null,
  })
}

beforeEach(() => {
  resetAnalyticsStore()
  useContextFeaturesStore.getState().reset()
  getAgentApiMock.mockReturnValue(agentApiMock)
  fetchContextUsageAnalyticsMock.mockReset()
  Object.values(agentApiMock).forEach((mock) => mock.mockReset())
})

describe('useAnalyticsStore', () => {
  test('shows beginner guidance when all primary analytics sources fail', async () => {
    agentApiMock.getAnalyticsSummary.mockResolvedValue({ ok: false })
    agentApiMock.getAnalyticsTools.mockResolvedValue({ ok: false, tools: [] })
    agentApiMock.getAnalyticsActivity.mockResolvedValue({ ok: false, activity: [] })
    agentApiMock.getAgents.mockResolvedValue({ ok: false, agents: [] })

    await useAnalyticsStore.getState().load()

    expect(useAnalyticsStore.getState().error).toBe(analyticsUnavailableMessage())
    expect(useAnalyticsStore.getState().error).toContain('Refresh the dashboard')
    expect(useAnalyticsStore.getState().error).toContain('new team space')
    expect(useAnalyticsStore.getState().error).toContain('run an agent task first')
    expect(useAnalyticsStore.getState().error).not.toContain('new workspace')
    expect(useAnalyticsStore.getState().error).not.toContain('API')
    expect(useAnalyticsStore.getState().loading).toBe(false)
  })

  test('keeps partial analytics visible when at least one source succeeds', async () => {
    agentApiMock.getAnalyticsSummary.mockResolvedValue({
      ok: true,
      totalEvents: 12,
      toolCalls: 4,
      prompts: 4,
      responses: 4,
      uniqueAgents: 2,
      timeSpanHours: 24,
    })
    agentApiMock.getAnalyticsTools.mockResolvedValue({ ok: false, tools: [] })
    agentApiMock.getAnalyticsActivity.mockResolvedValue({ ok: false, activity: [] })
    agentApiMock.getAgents.mockResolvedValue({ ok: false, agents: [] })

    await useAnalyticsStore.getState().load()

    expect(useAnalyticsStore.getState().error).toBeNull()
    expect(useAnalyticsStore.getState().summary?.totalEvents).toBe(12)
  })

  test('summarizes agent availability when the agent source succeeds', async () => {
    agentApiMock.getAnalyticsSummary.mockResolvedValue({ ok: false })
    agentApiMock.getAnalyticsTools.mockResolvedValue({ ok: false, tools: [] })
    agentApiMock.getAnalyticsActivity.mockResolvedValue({ ok: false, activity: [] })
    agentApiMock.getAgents.mockResolvedValue({
      ok: true,
      agents: [
        { id: 'agent-1', status: 'idle' },
        { id: 'agent-2', status: 'working' },
        { id: 'agent-3', status: 'offline' },
      ],
    })

    await useAnalyticsStore.getState().load()

    expect(useAnalyticsStore.getState().error).toBeNull()
    expect(useAnalyticsStore.getState().agentStats).toEqual({
      total: 3,
      online: 2,
      offline: 1,
      working: 1,
    })
  })

  test('shows connection guidance when analytics cannot initialize the API client', async () => {
    getAgentApiMock.mockImplementationOnce(() => {
      throw new Error('missing auth manager')
    })

    await useAnalyticsStore.getState().load()

    expect(useAnalyticsStore.getState().error).toBe(analyticsNetworkErrorMessage())
    expect(useAnalyticsStore.getState().error).toContain(
      'Check your connection, then refresh the dashboard'
    )
    expect(useAnalyticsStore.getState().error).not.toContain('missing auth manager')
    expect(useAnalyticsStore.getState().loading).toBe(false)
  })
})
