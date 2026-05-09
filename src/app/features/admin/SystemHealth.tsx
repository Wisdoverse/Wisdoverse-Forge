import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type ComponentHealth } from '@app/shared/model/admin.store'

// ============================================================================
// Status badge
// ============================================================================

type ServiceStatus = ComponentHealth['status'] | 'unknown'

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

// ============================================================================
// Service row
// ============================================================================

interface ServiceRowProps {
  name: string
  description: string
  health: ComponentHealth | undefined
}

function ServiceRow({ name, description, health }: ServiceRowProps) {
  const status: ServiceStatus = health?.status ?? 'unknown'

  return (
    <div className={cn('flex items-center justify-between px-4 py-3', uiStyles.row)}>
      <div className="flex items-center gap-3 min-w-0">
        <StatusDot status={status} />
        <div className="min-w-0">
          <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
            {name}
          </p>
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {description}
          </p>
        </div>
      </div>
      <div className="flex items-center gap-4 shrink-0 ml-4">
        {health?.latencyMs !== undefined && (
          <span className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
            {health.latencyMs}ms
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

function OverallBanner({ status }: { status: 'healthy' | 'degraded' | 'unhealthy' }) {
  const config = {
    healthy: {
      bg: 'border-apple-blue/20 bg-apple-blue/10',
      text: 'text-apple-blue',
      label: 'All systems operational',
    },
    degraded: {
      bg: 'border-black/[0.08] bg-black/[0.03] dark:border-white/[0.08] dark:bg-white/[0.03]',
      text: 'text-secondary-light dark:text-secondary-dark',
      label: 'Partial degradation',
    },
    unhealthy: {
      bg: 'border-apple-red/20 bg-apple-red/10',
      text: 'text-apple-red',
      label: 'System unhealthy',
    },
  }
  const c = config[status]

  return (
    <div className={cn('mb-6 flex items-center gap-2 rounded-card border px-4 py-3', c.bg)}>
      <span
        className={cn(
          'w-2.5 h-2.5 rounded-full',
          status === 'healthy'
            ? 'bg-apple-blue'
            : status === 'degraded'
              ? 'bg-[#86868b]'
              : 'bg-apple-red'
        )}
      />
      <span className={cn('text-ui-body font-medium', c.text)}>{c.label}</span>
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
    // Refresh every 30 seconds
    const interval = setInterval(() => void loadHealth(), 30_000)
    return () => clearInterval(interval)
  }, [loadHealth])

  const services = [
    { key: 'database', name: 'Database', description: 'PostgreSQL — primary data store' },
    { key: 'redis', name: 'Redis', description: 'Cache and pub/sub' },
    { key: 'nats', name: 'NATS', description: 'JetStream message queue' },
    { key: 'platform', name: 'Platform (gRPC)', description: 'Container orchestration service' },
    { key: 'bullmq', name: 'BullMQ', description: 'Background job processing' },
  ] as const

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>System Health</h2>
          <p className={uiStyles.sectionDescription}>Auto-refreshes every 30 seconds</p>
        </div>
        <button
          type="button"
          onClick={() => void loadHealth()}
          disabled={healthLoading}
          className={uiStyles.secondaryButton}
        >
          {healthLoading ? 'Refreshing...' : 'Refresh'}
        </button>
      </div>

      {/* Error */}
      {healthError && !health && <div className={uiStyles.error}>{healthError}</div>}

      {/* Loading skeleton */}
      {healthLoading && !health && (
        <div className="flex items-center justify-center py-12">
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">Loading...</p>
        </div>
      )}

      {/* Content */}
      {health && (
        <>
          <OverallBanner status={health.status} />

          <div className={cn(uiStyles.card)}>
            {services.map(({ key, name, description }) => (
              <ServiceRow
                key={key}
                name={name}
                description={description}
                health={health.checks[key]}
              />
            ))}
          </div>

          {/* Uptime */}
          {health.uptime !== undefined && (
            <p className="mt-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Server uptime:{' '}
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
