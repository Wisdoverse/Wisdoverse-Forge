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

function metricLabel(metric: string): string {
  switch (metric.toLowerCase()) {
    case 'agents':
      return 'Agents'
    case 'events':
      return 'Events'
    case 'tokens':
      return 'Tokens'
    default:
      return metric.charAt(0).toUpperCase() + metric.slice(1)
  }
}

function barColor(pct: number): string {
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
  const pct = Math.min(100, Math.round(metric.percentUsed))
  const isHigh = pct >= 80

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between">
        <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {metricLabel(metric.metric)}
        </span>
        <span
          className={cn(
            'text-ui-caption',
            isHigh
              ? 'font-semibold text-foreground-light dark:text-foreground-dark'
              : 'text-secondary-light dark:text-secondary-dark'
          )}
        >
          {formatNumber(metric.current)} / {formatNumber(metric.limit)}
        </span>
      </div>
      <div className="h-1.5 rounded-full bg-black/10 dark:bg-white/10 overflow-hidden">
        <div
          className={cn('h-full rounded-full transition-all', barColor(pct))}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        {pct}% used
      </span>
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
