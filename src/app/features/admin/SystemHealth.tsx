import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  useAdminStore,
  type ComponentHealth,
  type SystemHealth,
} from '@app/shared/model/admin.store'

// ============================================================================
// Status badge
// ============================================================================

type ServiceStatus = ComponentHealth['status'] | 'unknown'
type OverallStatus = 'healthy' | 'degraded' | 'unhealthy'

interface ServiceDefinition {
  key: keyof SystemHealth['checks']
  name: string
  supportName: string
  description: string
  impact: string
  action: string
}

const SERVICE_DEFINITIONS: readonly ServiceDefinition[] = [
  {
    key: 'database',
    name: 'Saved Data',
    supportName: 'PostgreSQL database',
    description: 'Keeps accounts, tasks, runs, evidence, and settings available.',
    impact: 'New work may fail to save, and existing pages may load slowly or not at all.',
    action: 'Check the database service first, then refresh this page.',
  },
  {
    key: 'redis',
    name: 'Fast Loading',
    supportName: 'Redis cache',
    description: 'Helps the app respond quickly and keeps short-lived coordination state.',
    impact: 'The app can still work, but pages and realtime updates may feel slower.',
    action: 'Wait a minute and refresh. If it stays degraded, restart the cache service.',
  },
  {
    key: 'nats',
    name: 'Live Updates',
    supportName: 'NATS message bus',
    description: 'Moves events from running agents into the browser in near real time.',
    impact: 'Runs may continue, but users may not see progress updates immediately.',
    action: 'Check the messaging service, then confirm new events appear after refresh.',
  },
  {
    key: 'platform',
    name: 'Agent Runner',
    supportName: 'Container platform',
    description: 'Starts and manages agent work sessions.',
    impact: 'Starting new agent work may fail until the runner is healthy again.',
    action: 'Check the runner service and container host before sending new work.',
  },
  {
    key: 'bullmq',
    name: 'Background Jobs',
    supportName: 'Job worker',
    description: 'Runs delayed and background work outside the main page request.',
    impact: 'Queued work may wait longer before it starts.',
    action: 'Check workers and retry the job after the queue is healthy.',
  },
]

function StatusDot({ status }: { status: ServiceStatus }) {
  const color =
    status === 'up'
      ? 'bg-apple-blue'
      : status === 'degraded'
        ? 'bg-[#86868b]'
        : status === 'down'
          ? 'bg-apple-red'
          : 'bg-gray-400'

  return <span className={cn('w-2 h-2 rounded-full flex-shrink-0', color)} />
}

function StatusBadge({ status }: { status: ServiceStatus }) {
  const styles: Record<ServiceStatus, string> = {
    up: 'bg-apple-blue/10 text-apple-blue',
    degraded: 'bg-black/[0.05] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark',
    down: 'bg-apple-red/10 text-apple-red',
    unknown: 'bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400',
  }
  const label =
    status === 'up'
      ? 'Healthy'
      : status === 'degraded'
        ? 'Degraded'
        : status === 'down'
          ? 'Down'
          : 'Unknown'

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-ui-caption font-medium',
        styles[status]
      )}
    >
      <StatusDot status={status} />
      {label}
    </span>
  )
}

function serviceStatusText(status: ServiceStatus): string {
  if (status === 'up') return 'Working normally'
  if (status === 'degraded') return 'Needs attention'
  if (status === 'down') return 'Not working'
  return 'Not checked yet'
}

function serviceTone(status: ServiceStatus): string {
  if (status === 'up') return 'text-apple-blue'
  if (status === 'down') return 'text-apple-red'
  return 'text-secondary-light dark:text-secondary-dark'
}

function countAttentionServices(health: SystemHealth) {
  return SERVICE_DEFINITIONS.filter((service) => {
    const status = health.checks[service.key]?.status ?? 'unknown'
    return status !== 'up'
  }).length
}

// ============================================================================
// Service row
// ============================================================================

interface ServiceRowProps extends ServiceDefinition {
  health: ComponentHealth | undefined
}

function ServiceRow({ name, supportName, description, impact, action, health }: ServiceRowProps) {
  const status: ServiceStatus = health?.status ?? 'unknown'
  const hasIssue = status !== 'up'

  return (
    <div className={cn('grid gap-3 px-4 py-3 sm:grid-cols-[1fr_auto]', uiStyles.row)}>
      <div className="flex min-w-0 gap-3">
        <StatusDot status={status} />
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              {name}
            </p>
            <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {supportName}
            </span>
          </div>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {description}
          </p>
          <p className={cn('mt-1 text-ui-caption font-medium', serviceTone(status))}>
            {serviceStatusText(status)}
          </p>
          {hasIssue && (
            <div className="mt-2 grid gap-1 rounded-card border border-black/[0.06] bg-black/[0.02] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.03]">
              <p className="text-ui-caption text-foreground-light dark:text-foreground-dark">
                User impact: {impact}
              </p>
              <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                Next step: {action}
              </p>
              {health?.error && (
                <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Reported detail: {health.error}
                </p>
              )}
            </div>
          )}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-4 sm:justify-end">
        {health?.latencyMs !== undefined && (
          <span className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
            Response: {health.latencyMs}ms
          </span>
        )}
        <StatusBadge status={status} />
      </div>
    </div>
  )
}

// ============================================================================
// Overall status banner
// ============================================================================

function OverallBanner({
  status,
  attentionCount,
}: {
  status: OverallStatus
  attentionCount: number
}) {
  const config = {
    healthy: {
      bg: 'border-apple-blue/20 bg-apple-blue/10',
      text: 'text-apple-blue',
      label: 'Everything is working',
      detail: 'Users should be able to open the app, start agent work, and see updates normally.',
    },
    degraded: {
      bg: 'border-black/[0.08] bg-black/[0.03] dark:border-white/[0.08] dark:bg-white/[0.03]',
      text: 'text-secondary-light dark:text-secondary-dark',
      label: 'Some parts need attention',
      detail:
        attentionCount === 1
          ? 'One area may be slower or unreliable. Review the next step below.'
          : `${attentionCount} areas may be slower or unreliable. Review the next steps below.`,
    },
    unhealthy: {
      bg: 'border-apple-red/20 bg-apple-red/10',
      text: 'text-apple-red',
      label: 'Users may be blocked',
      detail:
        attentionCount === 1
          ? 'One required area is not healthy. Fix it before asking users to retry.'
          : `${attentionCount} required areas are not healthy. Fix them before asking users to retry.`,
    },
  }
  const c = config[status]

  return (
    <div className={cn('mb-6 flex items-start gap-3 rounded-card border px-4 py-3', c.bg)}>
      <span
        className={cn(
          'mt-1 h-2.5 w-2.5 rounded-full',
          status === 'healthy'
            ? 'bg-apple-blue'
            : status === 'degraded'
              ? 'bg-[#86868b]'
              : 'bg-apple-red'
        )}
      />
      <div>
        <p className={cn('text-ui-body font-medium', c.text)}>{c.label}</p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {c.detail}
        </p>
      </div>
    </div>
  )
}

function formatUptime(seconds: number): string {
  if (seconds < 60) {
    const rounded = Math.round(seconds)
    return `${rounded} ${rounded === 1 ? 'second' : 'seconds'}`
  }
  if (seconds < 3600) {
    const rounded = Math.round(seconds / 60)
    return `${rounded} ${rounded === 1 ? 'minute' : 'minutes'}`
  }
  const rounded = Math.round(seconds / 3600)
  return `${rounded} ${rounded === 1 ? 'hour' : 'hours'}`
}

// ============================================================================
// SystemHealth
// ============================================================================

export function SystemHealth() {
  const { health, healthLoading, healthError, loadHealth } = useAdminStore()

  useEffect(() => {
    void loadHealth()
    // Refresh every 30 seconds
    const interval = setInterval(() => void loadHealth(), 30_000)
    return () => clearInterval(interval)
  }, [loadHealth])

  const attentionCount = health ? countAttentionServices(health) : 0

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>System Health</h2>
          <p className={uiStyles.sectionDescription}>
            Check whether users can save work, start agents, and see live updates.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadHealth()}
          disabled={healthLoading}
          className={uiStyles.secondaryButton}
        >
          {healthLoading ? 'Checking...' : 'Check Again'}
        </button>
      </div>

      {/* Error */}
      {healthError && !health && (
        <div role="alert" className={uiStyles.error}>
          Health could not be loaded. Check that the API is running, then try again. Detail:{' '}
          {healthError}
        </div>
      )}

      {/* Loading skeleton */}
      {healthLoading && !health && (
        <div className="flex items-center justify-center py-12">
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            Checking system health...
          </p>
        </div>
      )}

      {/* Content */}
      {health && (
        <>
          <OverallBanner status={health.status} attentionCount={attentionCount} />

          <div className={cn(uiStyles.card)}>
            {SERVICE_DEFINITIONS.map((service) => (
              <ServiceRow
                key={service.key}
                name={service.name}
                supportName={service.supportName}
                description={service.description}
                impact={service.impact}
                action={service.action}
                health={health.checks[service.key]}
              />
            ))}
          </div>

          {/* Uptime */}
          {health.uptime !== undefined && (
            <p className="mt-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
              The system has been running for about {formatUptime(health.uptime)}. This page checks
              again every 30 seconds.
            </p>
          )}
        </>
      )}
    </div>
  )
}
