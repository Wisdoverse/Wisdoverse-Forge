import { useMemo, useState } from 'react'
import { Activity, AlertTriangle, CheckCircle2, CircleDot, ListFilter } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useFeedStore } from '@app/shared/model/feed.store'
import { AgentStatusBar } from './AgentStatusBar'
import { AttentionZone } from './AttentionZone'
import { FeedItem } from './FeedItem'

type FeedFilter = 'all' | 'needs-action' | 'progress' | 'completed'

const FEED_FILTERS: { value: FeedFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'needs-action', label: 'Needs action' },
  { value: 'progress', label: 'Progress' },
  { value: 'completed', label: 'Completed' },
]

export function ActivityFeed() {
  const agents = useFeedStore((state) => state.agents)
  const attentionItems = useFeedStore((state) => state.attentionItems)
  const feedItems = useFeedStore((state) => state.feedItems)
  const [activeFilter, setActiveFilter] = useState<FeedFilter>('all')

  const operations = useMemo(() => {
    const workingAgents = agents.filter((agent) => agent.status === 'working').length
    const unavailableAgents = agents.filter(
      (agent) => agent.status === 'blocked' || agent.status === 'offline'
    ).length
    const failedUpdates = feedItems.filter(
      (item) => item.type === 'task.blocked' || item.type === 'task.failed'
    ).length
    const completedUpdates = feedItems.filter((item) => item.type === 'task.completed').length

    return {
      workingAgents,
      needsAction: attentionItems.length + unavailableAgents + failedUpdates,
      recentUpdates: feedItems.length,
      completedUpdates,
    }
  }, [agents, attentionItems.length, feedItems])

  const filterCounts = useMemo(
    () =>
      FEED_FILTERS.map((filter) => ({
        ...filter,
        count: countFilterItems(feedItems, filter.value),
      })),
    [feedItems]
  )

  const visibleFeedItems = useMemo(
    () => feedItems.filter((item) => feedItemMatchesFilter(item.type, activeFilter)),
    [activeFilter, feedItems]
  )

  return (
    <div className="flex flex-col gap-3">
      <section
        data-testid="feed-ops-summary"
        className="rounded-lg border border-black/[0.08] bg-white/70 p-3 dark:border-white/[0.1] dark:bg-white/[0.04]"
      >
        <div className="mb-3 flex items-center gap-2">
          <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-apple-blue/[0.1] text-apple-blue">
            <Activity size={15} strokeWidth={2.1} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <div className="truncate text-xs font-semibold text-foreground-light dark:text-foreground-dark">
              Agent operations
            </div>
            <div className="text-[10px] text-secondary-light dark:text-secondary-dark">
              Managed runtime status
            </div>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2">
          <SummaryMetric
            label="Working"
            value={operations.workingAgents}
            Icon={CircleDot}
            tone="working"
            testId="feed-metric-working"
          />
          <SummaryMetric
            label="Needs action"
            value={operations.needsAction}
            Icon={AlertTriangle}
            tone="attention"
            testId="feed-metric-needs-action"
          />
          <SummaryMetric
            label="Updates"
            value={operations.recentUpdates}
            Icon={ListFilter}
            tone="neutral"
            testId="feed-metric-updates"
          />
          <SummaryMetric
            label="Completed"
            value={operations.completedUpdates}
            Icon={CheckCircle2}
            tone="complete"
            testId="feed-metric-completed"
          />
        </div>
      </section>

      <AgentStatusBar agents={agents} />
      <AttentionZone items={attentionItems} />

      {feedItems.length > 0 ? (
        <div>
          <div className="mb-2 flex flex-col gap-2">
            <div className="text-[10px] font-semibold uppercase tracking-[0.08em] text-secondary-light dark:text-secondary-dark">
              Live feed
            </div>
            <div
              className="inline-flex max-w-full items-center gap-1 overflow-x-auto rounded-lg bg-black/[0.035] p-1 dark:bg-white/[0.05]"
              role="group"
              aria-label="Feed filter"
              data-testid="feed-filter-group"
            >
              {filterCounts.map((filter) => (
                <FeedFilterButton
                  key={filter.value}
                  active={activeFilter === filter.value}
                  label={filter.label}
                  count={filter.count}
                  onClick={() => setActiveFilter(filter.value)}
                />
              ))}
            </div>
          </div>
          {visibleFeedItems.length > 0 ? (
            <div className="flex flex-col divide-y divide-black/[0.04] dark:divide-white/[0.04]">
              {visibleFeedItems.map((item) => (
                <FeedItem key={item.id} item={item} />
              ))}
            </div>
          ) : (
            <FilteredEmptyState />
          )}
        </div>
      ) : (
        <div className="flex flex-col items-center text-center gap-2 py-10 px-2">
          <div className="w-10 h-10 rounded-full bg-black/[0.04] dark:bg-white/[0.06] text-secondary-light dark:text-secondary-dark flex items-center justify-center">
            <Activity size={18} strokeWidth={1.75} />
          </div>
          <p className="text-xs font-medium text-foreground-light dark:text-foreground-dark">
            Quiet so far
          </p>
          <p className="text-[11px] text-secondary-light dark:text-secondary-dark leading-relaxed">
            Tool calls, prompts, and agent updates will stream here in real time.
          </p>
        </div>
      )}
    </div>
  )
}

function SummaryMetric({
  label,
  value,
  Icon,
  tone,
  testId,
}: {
  label: string
  value: number
  Icon: typeof Activity
  tone: 'working' | 'attention' | 'neutral' | 'complete'
  testId: string
}) {
  return (
    <div
      data-testid={testId}
      className="min-w-0 rounded-lg bg-black/[0.025] px-2.5 py-2 dark:bg-white/[0.045]"
    >
      <div className="mb-1 flex items-center gap-1.5 text-secondary-light dark:text-secondary-dark">
        <Icon
          size={12}
          strokeWidth={2.1}
          aria-hidden="true"
          className={cn({
            'text-apple-green': tone === 'working' || tone === 'complete',
            'text-apple-red': tone === 'attention',
            'text-apple-blue': tone === 'neutral',
          })}
        />
        <span className="truncate text-[9px] font-medium uppercase tracking-wide">{label}</span>
      </div>
      <div className="text-sm font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
        {value}
      </div>
    </div>
  )
}

function FeedFilterButton({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean
  label: string
  count: number
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        'inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-[10px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
        active
          ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
          : 'text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark'
      )}
    >
      <span>{label}</span>
      <span className="tabular-nums text-secondary-light dark:text-secondary-dark">{count}</span>
    </button>
  )
}

function FilteredEmptyState() {
  return (
    <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-black/[0.08] px-3 py-6 text-center dark:border-white/[0.1]">
      <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
        <ListFilter size={15} strokeWidth={1.9} aria-hidden="true" />
      </div>
      <p className="text-[11px] font-medium text-foreground-light dark:text-foreground-dark">
        No updates in this view
      </p>
    </div>
  )
}

function countFilterItems(items: { type: string }[], filter: FeedFilter): number {
  if (filter === 'all') return items.length
  return items.filter((item) => feedItemMatchesFilter(item.type, filter)).length
}

function feedItemMatchesFilter(type: string, filter: FeedFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'needs-action') return type === 'task.blocked' || type === 'task.failed'
  if (filter === 'completed') return type === 'task.completed'
  return type === 'task.working' || type === 'task.progress' || type === 'task.queued'
}
