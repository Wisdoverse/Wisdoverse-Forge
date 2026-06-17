import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  useAdminStore,
  type ComponentHealth,
  type SystemHealth,
} from '@app/shared/model/admin.store'
import { systemHealthErrorMessage } from './systemHealthErrorMessage'

const SYSTEM_HEALTH_REFRESH_MS = 30_000

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
    supportName: 'Keeps saved work available',
    description: 'Keeps accounts, tasks, work history, saved work records, and settings available.',
    impact: 'New work may fail to save, and existing pages may load slowly or not at all.',
    action: 'Ask an owner or admin to check saved data first, then choose Check now.',
  },
  {
    key: 'redis',
    name: 'Fast Loading',
    supportName: 'Helps pages load quickly',
    description: 'Helps pages respond quickly and keeps temporary app state in sync.',
    impact: 'The app can still work, but pages and realtime updates may feel slower.',
    action:
      'Wait a minute, then choose Check now. If it still shows Check soon, ask an owner or admin to restart fast loading.',
  },
  {
    key: 'nats',
    name: 'Live Updates',
    supportName: 'Shows new progress in the browser',
    description: 'Moves events from running agents into the browser in near real time.',
    impact: 'Runs may continue, but users may not see progress updates immediately.',
    action: 'Ask an owner or admin to check live updates, then refresh and look for new progress.',
  },
  {
    key: 'docker',
    name: 'Agent Work Starter',
    supportName: 'Starts file-work agents',
    description: 'Starts and manages the prepared workspaces where agents do file work.',
    impact:
      'Starting new agents in managed workspaces may fail; agents already running may stop reporting.',
    action:
      'Ask an owner or admin to check managed workspace setup before sending new file work to agents.',
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
      ? 'Ready'
      : status === 'degraded'
        ? 'Check soon'
        : status === 'down'
          ? 'Fix first'
          : 'Check now'

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
  if (status === 'degraded') return 'Check soon'
  if (status === 'down') return 'Not working'
  return 'Choose Check now to confirm'
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

function serviceIssueNote(error: string): string {
  const detail = error.toLowerCase()

  if (
    detail.includes('password') ||
    detail.includes('token') ||
    detail.includes('secret') ||
    detail.includes('credential') ||
    detail.includes('unauthorized') ||
    detail.includes('forbidden') ||
    detail.includes('permission')
  ) {
    return 'This area reported an access setup problem. Ask an owner or admin to check saved data access, then choose Check now.'
  }
  if (
    detail.includes('connection') ||
    detail.includes('refused') ||
    detail.includes('unreachable') ||
    detail.includes('timeout') ||
    detail.includes('timed out')
  ) {
    return 'This area reported a connection problem. Use the next step above, then choose Check now.'
  }
  if (
    detail.includes('missing') ||
    detail.includes('not configured') ||
    detail.includes('configuration') ||
    detail.includes('config')
  ) {
    return 'A required setting may be missing. Ask an owner or admin to check app setup, then choose Check now.'
  }
  if (
    detail.includes('rate limit') ||
    detail.includes('too many') ||
    detail.includes('busy') ||
    detail.includes('overloaded')
  ) {
    return 'This area is busy. Wait a minute, then choose Check now.'
  }

  return 'This area reported a problem. Use the next step above, then choose Check now.'
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
  const responseTime = health?.latencyMs !== undefined ? `responds in ${health.latencyMs} ms` : null

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
                  Owner/admin note: {serviceIssueNote(health.error)}
                </p>
              )}
            </div>
          )}
        </div>
      </div>
      <div className="flex items-center gap-4 shrink-0 ml-4">
        {responseTime && (
          <span className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
            {responseTime}
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
  attentionCount: _attentionCount,
}: {
  status: OverallStatus
  attentionCount: number
}) {
  const config = {
    healthy: {
      bg: 'border-apple-blue/20 bg-apple-blue/10',
      text: 'text-apple-blue',
      label: 'All areas are ready',
      detail: 'Users should be able to open the app, run agents, and receive updates.',
    },
    degraded: {
      bg: 'border-black/[0.08] bg-black/[0.03] dark:border-white/[0.08] dark:bg-white/[0.03]',
      text: 'text-secondary-light dark:text-secondary-dark',
      label: 'Some areas need a check',
      detail:
        'Users may see slow screens, delayed updates, or work waiting to start until this clears.',
    },
    unhealthy: {
      bg: 'border-apple-red/20 bg-apple-red/10',
      text: 'text-apple-red',
      label: 'App interruption likely',
      detail: 'Fix the area marked Fix first before assigning new work.',
    },
  }
  const c = config[status]

  return (
    <div className={cn('mb-6 flex items-start gap-3 rounded-card border px-4 py-3', c.bg)}>
      <span
        className={cn(
          'mt-1 w-2.5 h-2.5 rounded-full',
          status === 'healthy'
            ? 'bg-apple-blue'
            : status === 'degraded'
              ? 'bg-[#86868b]'
              : 'bg-apple-red'
        )}
      />
      <div className="min-w-0">
        <p className={cn('text-ui-body font-medium', c.text)}>{c.label}</p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {c.detail}
        </p>
      </div>
    </div>
  )
}

// ============================================================================
// SystemHealth
// ============================================================================

export function SystemHealth() {
  const { health, healthLoading, healthError, loadHealth } = useAdminStore()

  useEffect(() => {
    void loadHealth()
    const interval = setInterval(() => {
      if (document.visibilityState === 'hidden') return
      void loadHealth()
    }, SYSTEM_HEALTH_REFRESH_MS)
    return () => clearInterval(interval)
  }, [loadHealth])

  const attentionCount = health ? countAttentionServices(health) : 0

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>App health check</h2>
          <p className={uiStyles.sectionDescription}>
            Checks when opened, then refreshes every 30 seconds while Admin is open. Start with
            anything marked Fix first, then items marked Check soon.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadHealth()}
          disabled={healthLoading}
          className={uiStyles.secondaryButton}
        >
          {healthLoading ? 'Checking now' : 'Check now'}
        </button>
      </div>

      {/* Error */}
      {healthError && !health && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {systemHealthErrorMessage(healthError)}
        </div>
      )}

      {/* Loading skeleton */}
      {healthLoading && !health && (
        <div className="flex items-center justify-center py-12">
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            Checking app health now
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
              Forge has been running for{' '}
              {health.uptime < 60
                ? `${Math.round(health.uptime)}s`
                : health.uptime < 3600
                  ? `${Math.round(health.uptime / 60)}m`
                  : `${Math.round(health.uptime / 3600)}h`}
            </p>
          )}
        </>
      )}
    </div>
  )
}
