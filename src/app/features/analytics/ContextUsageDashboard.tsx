import { AlertTriangle, Brain, CheckCircle2, Clock3, ShieldAlert, WandSparkles } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import type { ContextUsageAnalytics, ContextUsageItem } from '@app/shared/api/orchestration'
import { StatCard } from './StatCard'

interface ContextUsageDashboardProps {
  data: ContextUsageAnalytics | null
  loading?: boolean
}

const percent = (value: number): string => `${Math.round(value * 100)}%`

const ITEM_KIND_LABELS: Record<ContextUsageItem['itemKind'], string> = {
  memory: 'Saved memory',
  skill: 'Reusable skill',
}

const RUNTIME_LABELS: Record<string, string> = {
  api: 'Model service',
  container: 'Managed workspace',
  provider: 'Model service',
  local: 'Local agent',
  cli: 'This computer',
}

const TASK_KIND_LABELS: Record<string, string> = {
  chat: 'Chat task',
  coding: 'Code change',
  implementation: 'Implementation task',
  planning: 'Planning task',
  review: 'Review task',
  workflow: 'Workflow task',
}

const EMPTY_TOP_USEFUL = {
  title: 'No useful context yet',
  detail: 'Helpful items appear after users mark applied context as useful.',
}

const EMPTY_NEEDS_REVIEW = {
  title: 'No review signals',
  detail: 'Items show here when feedback says context may be outdated, incorrect, or sensitive.',
}

const EMPTY_STALE = {
  title: 'No stale context',
  detail: 'Nothing has crossed the stale threshold for this workspace.',
}

function relativeAge(timestamp: string): string {
  const value = Date.parse(timestamp)
  if (Number.isNaN(value)) return 'unknown'
  const seconds = Math.max(0, Math.floor((Date.now() - value) / 1000))
  if (seconds < 3600) return `${Math.max(1, Math.floor(seconds / 60))}m ago`
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h ago`
  return `${Math.floor(seconds / 86_400)}d ago`
}

function humanizeMachineValue(value: string): string {
  const normalized = value
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[_-]+/g, ' ')

  if (!normalized) return 'Unknown'

  return normalized
    .split(/\s+/)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(' ')
}

function runtimeLabel(runtime: string): string {
  return RUNTIME_LABELS[runtime] ?? humanizeMachineValue(runtime)
}

function taskKindLabel(taskKind: string): string {
  return TASK_KIND_LABELS[taskKind] ?? humanizeMachineValue(taskKind)
}

export function ContextUsageDashboard({ data, loading = false }: ContextUsageDashboardProps) {
  return (
    <section data-testid="context-usage-dashboard" className="mt-6">
      <div className="mb-3 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            Context reuse
          </h2>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Snapshot refreshed {data ? relativeAge(data.lastRefreshedAt) : 'when data is available'}
          </p>
          <p className="mt-1 max-w-2xl text-ui-body text-secondary-light dark:text-secondary-dark">
            Use this panel to keep context that helps work finish and review items that may be
            outdated, incorrect, or too sensitive before agents reuse them.
          </p>
        </div>
        {data?.isStale && (
          <div
            data-testid="context-usage-stale-banner"
            className="inline-flex max-w-xl items-center gap-2 rounded-card border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            <AlertTriangle size={14} strokeWidth={2} aria-hidden="true" />
            <span>
              Snapshot is older than {data.staleAfterHours}h. Refresh analytics before acting on
              these numbers.
            </span>
          </div>
        )}
      </div>

      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard
          title="Applied"
          value={data?.summary.appliedCount ?? 0}
          subtitle="Times context was added to agent work."
          loading={loading}
          accent="blue"
        />
        <StatCard
          title="Success"
          value={data ? percent(data.summary.successRate) : '0%'}
          subtitle="Completed work after context was used."
          loading={loading}
          accent="blue"
        />
        <StatCard
          title="Useful"
          value={data?.summary.feedbackUsefulCount ?? 0}
          subtitle="Times users marked the context helpful."
          loading={loading}
          accent="blue"
        />
        <StatCard
          title="Needs Review"
          value={data?.summary.feedbackNegativeCount ?? 0}
          subtitle="Signals to check before reuse."
          loading={loading}
          accent="red"
        />
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <UsageList
          testId="context-usage-top-useful"
          title="Top useful"
          description="Keep these available; users marked them helpful after use."
          icon="useful"
          items={data?.topUseful ?? []}
          loading={loading}
          empty={EMPTY_TOP_USEFUL}
        />
        <UsageList
          testId="context-usage-needs-review"
          title="Needs review"
          description="Check these before reuse because feedback says they may be unsafe or wrong."
          icon="review"
          items={data?.needsReview ?? []}
          loading={loading}
          empty={EMPTY_NEEDS_REVIEW}
        />
        <UsageList
          testId="context-usage-stale-items"
          title="Stale"
          description="Verify these before agents rely on them again."
          icon="stale"
          items={data?.staleItems ?? []}
          loading={loading}
          empty={EMPTY_STALE}
        />
      </div>
    </section>
  )
}

function UsageList({
  testId,
  title,
  description,
  icon,
  items,
  loading,
  empty,
}: {
  testId: string
  title: string
  description: string
  icon: 'useful' | 'review' | 'stale'
  items: ContextUsageItem[]
  loading: boolean
  empty: {
    title: string
    detail: string
  }
}) {
  const Icon = icon === 'useful' ? CheckCircle2 : icon === 'review' ? ShieldAlert : Clock3

  return (
    <div
      data-testid={testId}
      className="rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]"
    >
      <div className="mb-3 flex items-start gap-2">
        <Icon
          size={15}
          strokeWidth={2}
          className={cn(
            'mt-0.5 shrink-0',
            icon === 'useful' && 'text-apple-blue',
            icon === 'review' && 'text-apple-red',
            icon === 'stale' && 'text-secondary-light dark:text-secondary-dark'
          )}
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
            {title}
          </p>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {description}
          </p>
        </div>
      </div>

      {loading ? (
        <div className="space-y-2">
          <div className="h-14 animate-pulse rounded-card bg-black/[0.04] dark:bg-white/[0.05]" />
          <div className="h-14 animate-pulse rounded-card bg-black/[0.04] dark:bg-white/[0.05]" />
        </div>
      ) : items.length === 0 ? (
        <div className="flex min-h-28 flex-col justify-center gap-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          <p className="font-medium text-foreground-light dark:text-foreground-dark">
            {empty.title}
          </p>
          <p>{empty.detail}</p>
        </div>
      ) : (
        <div className="space-y-2">
          {items.map((item) => (
            <UsageItem
              key={`${item.itemId}:${item.agentId}:${item.taskKind}:${item.runtime}`}
              item={item}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function UsageItem({ item }: { item: ContextUsageItem }) {
  const Icon = item.itemKind === 'memory' ? Brain : WandSparkles
  const itemKindLabel = ITEM_KIND_LABELS[item.itemKind]
  const negative = item.feedbackNegativeCount > 0

  return (
    <div
      data-testid="context-usage-item"
      className="rounded-card border border-black/[0.08] px-3 py-2 dark:border-white/[0.08]"
    >
      <div className="flex min-w-0 items-start gap-2">
        <Icon
          size={14}
          strokeWidth={2}
          className={cn(
            'mt-0.5 shrink-0',
            item.itemKind === 'memory'
              ? 'text-apple-blue'
              : 'text-secondary-light dark:text-secondary-dark'
          )}
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <p className="truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {item.itemTitle}
            </p>
            <span className="shrink-0 rounded-full bg-black/[0.04] px-1.5 py-0.5 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
              {itemKindLabel}
            </span>
          </div>
          <p className="mt-1 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
            {item.agentName} · {runtimeLabel(item.runtime)} · {taskKindLabel(item.taskKind)}
          </p>
        </div>
      </div>

      <div className="mt-2 grid grid-cols-4 gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        <Metric label="applied" value={item.appliedCount} />
        <Metric label="success" value={percent(item.successRate)} />
        <Metric label="useful" value={item.feedbackUsefulCount} />
        <Metric
          label="review"
          value={item.feedbackNegativeCount}
          className={negative ? 'text-apple-red' : undefined}
        />
      </div>
    </div>
  )
}

function Metric({
  label,
  value,
  className,
}: {
  label: string
  value: string | number
  className?: string
}) {
  return (
    <div className="min-w-0">
      <div
        className={cn(
          'truncate font-semibold tabular-nums text-foreground-light dark:text-foreground-dark',
          className
        )}
      >
        {value}
      </div>
      <div className="truncate">{label}</div>
    </div>
  )
}
