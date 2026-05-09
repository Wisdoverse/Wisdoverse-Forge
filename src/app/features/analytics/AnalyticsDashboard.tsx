import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { useAnalyticsStore, type DateRange } from '@app/shared/model/analytics.store'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { ContextUsageDashboard } from './ContextUsageDashboard'
import { StatCard, type BarPoint } from './StatCard'

const DATE_RANGE_OPTIONS: { value: DateRange; label: string }[] = [
  { value: 'today', label: 'Today' },
  { value: '7d', label: 'Last 7 days' },
  { value: '30d', label: 'Last 30 days' },
]

function hourlyToBars(hourly: { hour: number; count: number }[]): BarPoint[] {
  // Take up to 24 hours, group by hour slot
  return hourly.slice(-24).map((h) => ({
    label: `${h.hour}:00`,
    value: h.count,
  }))
}

function toolsToBars(tools: { tool: string; count: number }[]): BarPoint[] {
  return tools.slice(0, 8).map((t) => ({
    label: t.tool,
    value: t.count,
  }))
}

export function AnalyticsDashboard() {
  const {
    dateRange,
    summary,
    tools,
    hourly,
    agentStats,
    contextUsage,
    loading,
    error,
    setDateRange,
    load,
  } = useAnalyticsStore()
  const contextAnalyticsEnabled = useContextFeaturesStore((s) => s.analytics)

  useEffect(() => {
    void load()
  }, [load, dateRange])

  const activityBars = hourlyToBars(hourly)
  const toolBars = toolsToBars(tools)

  const topTool = tools[0]
  const topToolRate = topTool ? Math.round(topTool.successRate * 100) : 0

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center justify-end border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06] sm:px-6">
        {/* Date range selector */}
        <div className="flex items-center gap-0.5 rounded-full border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.06]">
          {DATE_RANGE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setDateRange(opt.value)}
              className={cn(
                'rounded-full px-3 py-1 text-ui-caption font-medium transition-transform active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                dateRange === opt.value
                  ? 'bg-apple-blue text-white'
                  : 'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark'
              )}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {error && (
          <div className="mb-4 rounded-card border border-apple-red/20 bg-apple-red/10 px-4 py-2 text-ui-body text-apple-red">
            {error}
          </div>
        )}

        {/* Agent status section */}
        <section className="mb-6">
          <h2 className="mb-3 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            Agents
          </h2>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <StatCard
              title="Total Agents"
              value={agentStats.total}
              loading={loading}
              accent="blue"
            />
            <StatCard
              title="Online"
              value={agentStats.online}
              subtitle={
                agentStats.total > 0
                  ? `${Math.round((agentStats.online / agentStats.total) * 100)}% of total`
                  : undefined
              }
              loading={loading}
              accent="blue"
            />
            <StatCard title="Working" value={agentStats.working} loading={loading} accent="blue" />
            <StatCard title="Offline" value={agentStats.offline} loading={loading} accent="red" />
          </div>
        </section>

        {/* Event metrics */}
        <section className="mb-6">
          <h2 className="mb-3 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            Activity
          </h2>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <StatCard
              title="Total Events"
              value={summary?.totalEvents ?? 0}
              loading={loading}
              accent="blue"
            />
            <StatCard
              title="Tool Calls"
              value={summary?.toolCalls ?? 0}
              loading={loading}
              accent="blue"
            />
            <StatCard
              title="Prompts"
              value={summary?.prompts ?? 0}
              loading={loading}
              accent="blue"
            />
            <StatCard
              title="Responses"
              value={summary?.responses ?? 0}
              loading={loading}
              accent="blue"
            />
          </div>
        </section>

        {/* Charts row */}
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          {/* Hourly activity */}
          <div className="rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]">
            <div className="mb-3 flex items-center justify-between">
              <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Hourly Activity
              </p>
            </div>
            {loading ? (
              <div className="h-24 animate-pulse rounded-card bg-black/[0.04] dark:bg-white/[0.05]" />
            ) : activityBars.length > 0 ? (
              <ActivityBarChart bars={activityBars} />
            ) : (
              <div className="flex h-24 items-center justify-center">
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  No activity data
                </p>
              </div>
            )}
          </div>

          {/* Top tools */}
          <div className="rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]">
            <p className="mb-3 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Top Tools
            </p>
            {loading ? (
              <div className="h-20 animate-pulse rounded-card bg-black/[0.04] dark:bg-white/[0.05]" />
            ) : toolBars.length > 0 ? (
              <div className="flex flex-col gap-1.5">
                {tools.slice(0, 5).map((t) => {
                  const maxCount = tools[0]?.count ?? 1
                  const pct = Math.max(2, Math.round((t.count / maxCount) * 100))
                  const rate = Math.round(t.successRate * 100)
                  return (
                    <div key={t.tool} className="flex items-center gap-2">
                      <span className="w-24 shrink-0 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                        {t.tool}
                      </span>
                      <div className="h-2 flex-1 overflow-hidden rounded-full bg-black/[0.04] dark:bg-white/[0.05]">
                        <div
                          className="h-full rounded-full bg-apple-blue/70 transition-[width]"
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                      <span className="w-8 shrink-0 text-right text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
                        {t.count}
                      </span>
                      <span
                        className={cn(
                          'w-10 shrink-0 text-right text-ui-caption tabular-nums',
                          rate >= 50
                            ? 'text-secondary-light dark:text-secondary-dark'
                            : 'text-apple-red'
                        )}
                      >
                        {rate}%
                      </span>
                    </div>
                  )
                })}
              </div>
            ) : (
              <div className="flex h-20 items-center justify-center">
                <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
                  No tool usage data
                </p>
              </div>
            )}
          </div>
        </div>

        {/* Top tool summary card */}
        {!loading && topTool && (
          <div className="mt-4">
            <StatCard
              title="Most Used Tool"
              value={topTool.tool}
              subtitle={`${topTool.count} calls · ${topToolRate}% success rate`}
              accent="blue"
            />
          </div>
        )}

        {contextAnalyticsEnabled && <ContextUsageDashboard data={contextUsage} loading={loading} />}
      </div>
    </div>
  )
}

// Simple bar chart for hourly activity. Highlights the most recent bucket,
// shows per-bar detail on hover/focus in the header row, and displays a
// sparse time axis (first · middle · last labels) beneath the bars.
function ActivityBarChart({ bars }: { bars: BarPoint[] }) {
  const [hoverIndex, setHoverIndex] = useState<number | null>(null)
  const max = Math.max(...bars.map((b) => b.value), 1)
  const total = bars.reduce((sum, b) => sum + b.value, 0)
  const lastIndex = bars.length - 1
  const activeIndex = hoverIndex ?? lastIndex
  const activeBar = bars[activeIndex]
  const activePct = total > 0 ? Math.round((activeBar.value / total) * 100) : 0

  // Pick 3 axis label positions: first, middle, last
  const middleIndex = Math.floor(lastIndex / 2)
  const labelIndices = new Set([0, middleIndex, lastIndex])

  return (
    <div
      className="flex flex-col"
      onMouseLeave={() => setHoverIndex(null)}
      data-testid="activity-chart"
    >
      {/* Detail header shows hovered bar or, by default, the most recent. */}
      <div className="mb-2 flex items-baseline justify-between text-ui-caption">
        <span
          data-testid="activity-chart-detail"
          className="font-medium tabular-nums text-foreground-light dark:text-foreground-dark"
        >
          {activeBar.label}
          <span className="mx-1.5 text-secondary-light dark:text-secondary-dark">·</span>
          {activeBar.value} events
        </span>
        <span className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
          {hoverIndex === null ? 'most recent' : `${activePct}% of window`}
        </span>
      </div>

      {/* Bars */}
      <div
        className="flex h-16 w-full items-end gap-px"
        role="group"
        aria-label="Hourly event activity"
      >
        {bars.map((bar, i) => {
          const heightPct = Math.max(2, (bar.value / max) * 100)
          const isActive = i === activeIndex
          const isLatest = i === lastIndex
          return (
            <button
              key={i}
              type="button"
              onMouseEnter={() => setHoverIndex(i)}
              onFocus={() => setHoverIndex(i)}
              onBlur={() => setHoverIndex(null)}
              aria-label={`${bar.label}: ${bar.value} events`}
              className="flex flex-1 cursor-default flex-col justify-end rounded-sm text-ui-button focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
            >
              <div
                className={cn(
                  'w-full rounded-t-sm transition-colors',
                  isActive
                    ? 'bg-apple-blue'
                    : isLatest
                      ? 'bg-apple-blue/80'
                      : 'bg-apple-blue/50 hover:bg-apple-blue/75'
                )}
                style={{ height: `${heightPct}%` }}
              />
            </button>
          )
        })}
      </div>

      {/* Time axis */}
      <div className="mt-1.5 flex gap-px text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
        {bars.map((bar, i) => (
          <span key={i} className="flex-1 text-center">
            {labelIndices.has(i) ? bar.label : ''}
          </span>
        ))}
      </div>
    </div>
  )
}
