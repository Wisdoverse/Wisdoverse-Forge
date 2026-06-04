import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { UsageMetric } from '@app/shared/api/legacy/billingApi'

// ============================================================================
// Helpers
// ============================================================================

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function readableMetric(metric: string): string {
  return metric
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/\b\w/g, (char) => char.toUpperCase())
}

function metricCopy(metric: string): { label: string; description: string; highAction: string } {
  switch (metric.toLowerCase()) {
    case 'agents':
      return {
        label: 'Agents',
        description: 'Managed work actors your team can run.',
        highAction: 'Archive unused agents or upgrade before creating more.',
      }
    case 'events':
      return {
        label: 'Activity events',
        description: 'Run updates, audit records, and timeline messages.',
        highAction: 'Export old records or plan for more capacity.',
      }
    case 'tokens':
      return {
        label: 'AI text usage',
        description: 'Text processed while agents work.',
        highAction: 'Review busy agents or upgrade before more runs are blocked.',
      }
    default:
      return {
        label: readableMetric(metric),
        description: 'Usage tracked by this plan.',
        highAction: 'Review this limit before starting more work.',
      }
  }
}

function usageStatus(pct: number, hasLimit: boolean): { label: string; color: string } {
  if (!hasLimit) {
    return {
      label: 'No limit set',
      color: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
    }
  }
  if (pct >= 100) return { label: 'Limit reached', color: 'bg-apple-red/10 text-apple-red' }
  if (pct >= 90) return { label: 'Almost full', color: 'bg-apple-red/10 text-apple-red' }
  if (pct >= 80) {
    return {
      label: 'Getting close',
      color: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
    }
  }
  return { label: 'Plenty left', color: 'bg-apple-blue/10 text-apple-blue' }
}

function barColor(pct: number, hasLimit: boolean): string {
  if (!hasLimit) return 'bg-[#86868b]'
  if (pct >= 90) return 'bg-apple-red'
  if (pct >= 80) return 'bg-[#86868b]'
  return 'bg-apple-blue'
}

// ============================================================================
// Single meter
// ============================================================================

interface MeterProps {
  metric: UsageMetric
}

function Meter({ metric }: MeterProps) {
  const hasLimit = metric.limit > 0
  const pct = hasLimit ? Math.max(0, Math.min(100, Math.round(metric.percentUsed))) : 0
  const copy = metricCopy(metric.metric)
  const status = usageStatus(pct, hasLimit)
  const isHigh = hasLimit && pct >= 80

  return (
    <div className="flex min-h-[144px] flex-col gap-2">
      <div className="flex items-center justify-between">
        <span className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
          {copy.label}
        </span>
        <span
          className={cn(
            'inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-ui-caption font-medium',
            status.color
          )}
        >
          {status.label}
        </span>
      </div>

      <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        {copy.description}
      </p>

      <div className="mt-auto flex items-center justify-between gap-3">
        <span
          className={cn(
            'text-ui-caption',
            isHigh
              ? 'font-semibold text-foreground-light dark:text-foreground-dark'
              : 'text-secondary-light dark:text-secondary-dark'
          )}
        >
          {hasLimit
            ? `${formatNumber(metric.current)} / ${formatNumber(metric.limit)}`
            : `${formatNumber(metric.current)} used`}
        </span>
        {hasLimit && (
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {pct}% used
          </span>
        )}
      </div>

      <div className="h-1.5 rounded-full bg-black/10 dark:bg-white/10 overflow-hidden">
        <div
          className={cn('h-full rounded-full transition-all', barColor(pct, hasLimit))}
          style={{ width: hasLimit ? `${pct}%` : '100%' }}
        />
      </div>

      {isHigh && (
        <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
          {copy.highAction}
        </p>
      )}
    </div>
  )
}

// ============================================================================
// UsageMeter
// ============================================================================

interface UsageMeterProps {
  metrics: UsageMetric[]
  loading?: boolean
}

export function UsageMeter({ metrics, loading }: UsageMeterProps) {
  if (loading) {
    return (
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
        {[0, 1, 2].map((i) => (
          <div key={i} className={cn(uiStyles.cardPadded, 'animate-pulse')}>
            <div className="h-4 w-20 bg-black/10 dark:bg-white/10 rounded mb-3" />
            <div className="h-2 w-full bg-black/10 dark:bg-white/10 rounded" />
          </div>
        ))}
      </div>
    )
  }

  if (metrics.length === 0) {
    return null
  }

  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
      {metrics.map((m) => (
        <div key={m.metric} className={uiStyles.cardPadded}>
          <Meter metric={m} />
        </div>
      ))}
    </div>
  )
}
