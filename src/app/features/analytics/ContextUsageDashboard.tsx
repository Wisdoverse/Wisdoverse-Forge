import { AlertTriangle, Brain, CheckCircle2, Clock3, ShieldAlert, WandSparkles } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import type { ContextUsageAnalytics, ContextUsageItem } from '@app/shared/api/orchestration'
import { StatCard } from './StatCard'

interface ContextUsageDashboardProps {
  data: ContextUsageAnalytics | null
  loading?: boolean
}

const percent = (value: number): string => `${Math.round(value * 100)}%`

const RUNTIME_LABELS: Record<string, string> = {
  api: 'Chat-only AI service',
  container: 'Managed workspace',
  provider: 'Chat-only AI service',
  local: 'This computer',
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
  title: 'Mark useful saved items to rank them here',
  detail:
    'After a task uses a saved note or instruction, choose Useful in the task result to place it in this list.',
}

const EMPTY_NEEDS_REVIEW = {
  title: 'Nothing to check right now',
  detail: 'Items appear here when people report they may be outdated, incorrect, or too sensitive.',
}

const EMPTY_STALE = {
  title: 'Nothing looks outdated',
  detail: 'Saved notes and saved instructions appear here when they are old enough to check again.',
}

function updatedAtLabel(timestamp: string): string {
  const value = Date.parse(timestamp)
  if (Number.isNaN(value)) return 'Refresh analytics to update time'
  const seconds = Math.max(0, Math.floor((Date.now() - value) / 1000))
  if (seconds < 3600) return `Updated ${Math.max(1, Math.floor(seconds / 60))}m ago`
  if (seconds < 86_400) return `Updated ${Math.floor(seconds / 3600)}h ago`
  return `Updated ${Math.floor(seconds / 86_400)}d ago`
}

function runtimeLabel(runtime: string): string {
  const normalized = runtime.trim().toLowerCase()
  if (!normalized) return 'Refresh work location'
  return RUNTIME_LABELS[normalized] ?? 'Check work location'
}

function taskKindLabel(taskKind: string): string {
  const normalized = taskKind.trim().toLowerCase()
  if (!normalized) return 'Task type not listed'
  return TASK_KIND_LABELS[normalized] ?? 'Task type needs review'
}

function contextItemKindLabel(itemKind: string): string {
  switch (itemKind) {
    case 'memory':
      return 'Saved note'
    case 'skill':
      return 'Saved instruction'
    default:
      return 'Saved item'
  }
}

export function ContextUsageDashboard({ data, loading = false }: ContextUsageDashboardProps) {
  return (
    <section data-testid="context-usage-dashboard" className="mt-6">
      <div className="mb-3 flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            Saved item reuse
          </h2>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            {data ? updatedAtLabel(data.lastRefreshedAt) : 'Updated when data is available'}
          </p>
          <p className="mt-1 max-w-2xl text-ui-body text-secondary-light dark:text-secondary-dark">
            Use this panel to keep saved notes and instructions that help work finish, and check
            items that may be outdated, incorrect, or too sensitive before agents reuse them.
          </p>
        </div>
        {data?.isStale && (
          <div
            data-testid="context-usage-stale-banner"
            className="inline-flex max-w-xl items-center gap-2 rounded-card border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-caption text-apple-red"
          >
            <AlertTriangle size={14} strokeWidth={2} aria-hidden="true" />
            <span>
              These numbers are more than {data.staleAfterHours}h old. Refresh analytics before
              making decisions from them.
            </span>
          </div>
        )}
      </div>

      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <StatCard
          title="Applied"
          value={data?.summary.appliedCount ?? 0}
          subtitle="Times saved notes or instructions were added to agent work."
          loading={loading}
          accent="blue"
        />
        <StatCard
          title="Success"
          value={data ? percent(data.summary.successRate) : '0%'}
          subtitle="Completed work after saved items were used."
          loading={loading}
          accent="blue"
        />
        <StatCard
          title="Useful"
          value={data?.summary.feedbackUsefulCount ?? 0}
          subtitle="Times users marked saved items helpful."
          loading={loading}
          accent="blue"
        />
        <StatCard
          title="Check first"
          value={data?.summary.feedbackNegativeCount ?? 0}
          subtitle="Items people marked for another look."
          loading={loading}
          accent="red"
        />
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-3">
        <UsageList
          testId="context-usage-top-useful"
          title="Top useful"
          description="Keep these available; users marked them helpful after use."
          nextStep="Next: keep this available for similar tasks."
          icon="useful"
          items={data?.topUseful ?? []}
          loading={loading}
          empty={EMPTY_TOP_USEFUL}
        />
        <UsageList
          testId="context-usage-needs-review"
          title="Check before reuse"
          description="Review these before reuse because people reported they may be outdated, incorrect, or sensitive."
          nextStep="Next: open the latest task result, then update or remove this before reuse."
          icon="review"
          items={data?.needsReview ?? []}
          loading={loading}
          empty={EMPTY_NEEDS_REVIEW}
        />
        <UsageList
          testId="context-usage-stale-items"
          title="May be outdated"
          description="Verify these before agents rely on them again."
          nextStep="Next: verify this still matches current team guidance before reuse."
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
  nextStep,
  icon,
  items,
  loading,
  empty,
}: {
  testId: string
  title: string
  description: string
  nextStep: string
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
              nextStep={nextStep}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function UsageItem({ item, nextStep }: { item: ContextUsageItem; nextStep: string }) {
  const Icon = item.itemKind === 'memory' ? Brain : WandSparkles
  const itemKindLabel = contextItemKindLabel(item.itemKind)
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
          label="check"
          value={item.feedbackNegativeCount}
          className={negative ? 'text-apple-red' : undefined}
        />
      </div>
      <p className="mt-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        {nextStep}
      </p>
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
