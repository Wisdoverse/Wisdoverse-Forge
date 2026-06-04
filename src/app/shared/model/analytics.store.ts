import { create } from 'zustand'
import { getAgentApi } from '@app/shared/api/legacy'
import { orchestrationApi, type ContextUsageAnalytics } from '@app/shared/api/orchestration'
import { useContextFeaturesStore } from './context-features.store'

export type DateRange = 'today' | '7d' | '30d'

export interface AnalyticsSummaryData {
  totalEvents: number
  toolCalls: number
  prompts: number
  responses: number
  uniqueAgents: number
  timeSpanHours: number
}

export interface ToolStat {
  tool: string
  count: number
  successRate: number
}

export interface HourlyPoint {
  hour: number
  count: number
}

export interface AgentStats {
  total: number
  online: number
  offline: number
  working: number
}

interface AnalyticsState {
  dateRange: DateRange
  summary: AnalyticsSummaryData | null
  tools: ToolStat[]
  hourly: HourlyPoint[]
  agentStats: AgentStats
  contextUsage: ContextUsageAnalytics | null
  loading: boolean
  error: string | null

  setDateRange: (range: DateRange) => void
  load: () => Promise<void>
}

function dateRangeToHours(range: DateRange): number {
  switch (range) {
    case 'today':
      return 24
    case '7d':
      return 168
    case '30d':
      return 720
  }
}

const initialState = {
  dateRange: 'today' as DateRange,
  summary: null as AnalyticsSummaryData | null,
  tools: [] as ToolStat[],
  hourly: [] as HourlyPoint[],
  agentStats: { total: 0, online: 0, offline: 0, working: 0 } as AgentStats,
  contextUsage: null as ContextUsageAnalytics | null,
  loading: false,
  error: null as string | null,
}

export function analyticsUnavailableMessage(): string {
  return 'Analytics could not load live activity. Refresh the dashboard. If this is a new workspace, run an agent task first so there is activity to report.'
}

export function analyticsNetworkErrorMessage(): string {
  return 'Analytics could not reach the server. Check your connection, then refresh the dashboard.'
}

export const useAnalyticsStore = create<AnalyticsState>((set, get) => ({
  ...initialState,

  setDateRange: (range) => set({ dateRange: range }),

  load: async () => {
    const { dateRange } = get()
    const hours = dateRangeToHours(dateRange)
    set({ loading: true, error: null })

    try {
      const api = getAgentApi()
      const params = { hours }
      const contextAnalyticsEnabled = useContextFeaturesStore.getState().analytics

      const [summaryRes, toolsRes, activityRes, agentsRes, contextUsageRes] =
        await Promise.allSettled([
          api.getAnalyticsSummary(params),
          api.getAnalyticsTools(params),
          api.getAnalyticsActivity(params),
          api.getAgents(),
          contextAnalyticsEnabled
            ? orchestrationApi.fetchContextUsageAnalytics({ limit: 8 })
            : null,
        ])

      let summary: AnalyticsSummaryData | null = null
      if (summaryRes.status === 'fulfilled' && summaryRes.value.ok) {
        const r = summaryRes.value
        summary = {
          totalEvents: r.totalEvents ?? 0,
          toolCalls: r.toolCalls ?? 0,
          prompts: r.prompts ?? 0,
          responses: r.responses ?? 0,
          uniqueAgents: r.uniqueAgents ?? 0,
          timeSpanHours: r.timeSpanHours ?? hours,
        }
      }

      let tools: ToolStat[] = []
      if (toolsRes.status === 'fulfilled' && toolsRes.value.ok) {
        tools = toolsRes.value.tools ?? []
      }

      let hourly: HourlyPoint[] = []
      if (activityRes.status === 'fulfilled' && activityRes.value.ok) {
        hourly = activityRes.value.activity ?? []
      }

      let agentStats: AgentStats = { total: 0, online: 0, offline: 0, working: 0 }
      if (agentsRes.status === 'fulfilled' && agentsRes.value.ok) {
        const agents = agentsRes.value.agents ?? []
        agentStats = agents.reduce(
          (acc, a) => {
            acc.total++
            if (a.status === 'working' || a.status === 'waiting' || a.status === 'attention') {
              acc.working++
              acc.online++
            } else if (a.status === 'idle') {
              acc.online++
            } else {
              acc.offline++
            }
            return acc
          },
          { total: 0, online: 0, offline: 0, working: 0 }
        )
      }

      const contextUsage = contextAnalyticsEnabled
        ? contextUsageRes.status === 'fulfilled'
          ? contextUsageRes.value
          : get().contextUsage
        : null

      const hasPrimaryDataSource = [summaryRes, toolsRes, activityRes, agentsRes].some(
        (result) => result.status === 'fulfilled' && result.value.ok
      )

      set({
        summary,
        tools,
        hourly,
        agentStats,
        contextUsage,
        loading: false,
        error: hasPrimaryDataSource ? null : analyticsUnavailableMessage(),
      })
    } catch {
      set({
        loading: false,
        error: analyticsNetworkErrorMessage(),
      })
    }
  },
}))
