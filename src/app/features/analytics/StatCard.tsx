import { cn } from '@app/shared/lib/utils'

export interface BarPoint {
  label: string
  value: number
}

interface StatCardProps {
  title: string
  value: string | number
  subtitle?: string
  trend?: 'up' | 'down' | 'neutral'
  trendLabel?: string
  bars?: BarPoint[]
  accent?: 'blue' | 'green' | 'orange' | 'red' | 'purple'
  loading?: boolean
}

const ACCENT_CLASSES: Record<NonNullable<StatCardProps['accent']>, string> = {
  blue: 'text-apple-blue',
  green: 'text-foreground-light dark:text-foreground-dark',
  orange: 'text-foreground-light dark:text-foreground-dark',
  red: 'text-apple-red',
  purple: 'text-foreground-light dark:text-foreground-dark',
}

const BAR_CLASSES: Record<NonNullable<StatCardProps['accent']>, string> = {
  blue: 'bg-apple-blue/70',
  green: 'bg-apple-blue/55',
  orange: 'bg-apple-blue/45',
  red: 'bg-apple-red/70',
  purple: 'bg-apple-blue/60',
}

function TrendArrow({ trend, label }: { trend: 'up' | 'down' | 'neutral'; label?: string }) {
  if (trend === 'neutral') return null
  return (
    <span
      className={cn(
        'inline-flex items-center gap-0.5 text-ui-caption font-medium',
        trend === 'up' ? 'text-apple-blue' : 'text-apple-red'
      )}
    >
      <span aria-hidden="true">{trend === 'up' ? '↑' : '↓'}</span>
      {label && <span>{label}</span>}
    </span>
  )
}

function MiniBarChart({
  bars,
  accent = 'blue',
}: {
  bars: BarPoint[]
  accent?: StatCardProps['accent']
}) {
  const max = Math.max(...bars.map((b) => b.value), 1)
  const barClass = BAR_CLASSES[accent ?? 'blue']

  return (
    <div className="flex h-8 items-end gap-0.5">
      {bars.map((bar) => (
        <div
          key={bar.label}
          className="group flex flex-1 flex-col items-center gap-0.5"
          title={`${bar.label}: ${bar.value}`}
        >
          <div
            className={cn('w-full rounded-sm transition-[height]', barClass)}
            style={{ height: `${Math.max(2, (bar.value / max) * 100)}%` }}
          />
        </div>
      ))}
    </div>
  )
}

export function StatCard({
  title,
  value,
  subtitle,
  trend,
  trendLabel,
  bars,
  accent = 'blue',
  loading = false,
}: StatCardProps) {
  const accentClass = ACCENT_CLASSES[accent]

  return (
    <div
      aria-busy={loading || undefined}
      className={cn(
        'flex flex-col gap-3 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]'
      )}
    >
      <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        {title}
      </p>

      {loading ? (
        <div className="flex min-h-7 items-center gap-2" aria-live="polite">
          <div
            className="h-7 w-20 animate-pulse rounded-card bg-black/[0.04] dark:bg-white/[0.05]"
            aria-hidden="true"
          />
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            Loading
          </span>
        </div>
      ) : (
        <div className="flex items-baseline gap-2">
          <span className={cn('text-ui-metric font-semibold tabular-nums', accentClass)}>
            {value}
          </span>
          {trend && trend !== 'neutral' && <TrendArrow trend={trend} label={trendLabel} />}
        </div>
      )}

      {subtitle && (
        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">{subtitle}</p>
      )}

      {bars && bars.length > 0 && !loading && <MiniBarChart bars={bars} accent={accent} />}
    </div>
  )
}
