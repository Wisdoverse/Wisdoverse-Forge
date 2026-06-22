import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type OrgControlPlaneSnapshot } from '@app/shared/model/admin.store'
import { controlPlaneErrorMessage } from './controlPlaneErrorMessage'

const CONTROL_PLANE_REFRESH_MS = 30_000

// ============================================================================
// Signal row
// ============================================================================

interface SignalRowProps {
  label: string
  value: number
  /** When true a non-zero value is rendered as a warning. */
  warnNonZero?: boolean
  /** Optional suffix shown after the value (e.g. 's'). */
  unit?: string
}

function SignalRow({ label, value, warnNonZero = false, unit = '' }: SignalRowProps) {
  const isWarning = warnNonZero && value !== 0
  const valueText = unit ? `${value}${unit}` : String(value)

  return (
    <div className={cn('flex items-center justify-between px-4 py-3', uiStyles.row)}>
      <p className="text-ui-body text-foreground-light dark:text-foreground-dark">{label}</p>
      <span
        className={cn(
          'ml-4 shrink-0 tabular-nums text-ui-body font-medium',
          isWarning ? 'text-apple-red' : 'text-foreground-light dark:text-foreground-dark'
        )}
        aria-label={
          isWarning ? `${label}: ${valueText}, needs attention` : `${label}: ${valueText}`
        }
      >
        {valueText}
      </span>
    </div>
  )
}

// ============================================================================
// Signal definitions
// ============================================================================

interface SignalDef {
  key: keyof OrgControlPlaneSnapshot
  label: string
  warnNonZero: boolean
  unit?: string
}

const SIGNAL_DEFS: readonly SignalDef[] = [
  {
    key: 'assignmentOutboxBacklog',
    label: 'Assignment updates waiting to send',
    warnNonZero: true,
  },
  {
    key: 'assignmentOutboxOldestAgeSeconds',
    label: 'Oldest waiting assignment update (s)',
    warnNonZero: true,
    unit: 's',
  },
  {
    key: 'staleParticipants',
    label: 'Agents not checking in',
    warnNonZero: true,
  },
  {
    key: 'expiredWorkingLeases',
    label: 'Work check-ins overdue',
    warnNonZero: true,
  },
  {
    key: 'busyParticipantsWithoutWork',
    label: 'Busy agents without work',
    warnNonZero: true,
  },
  {
    key: 'workingTasksWithoutBusyParticipant',
    label: 'Working tasks without a busy agent',
    warnNonZero: true,
  },
]

// ============================================================================
// ControlPlanePanel
// ============================================================================

export function ControlPlanePanel() {
  const { controlPlane, controlPlaneLoading, controlPlaneError, loadControlPlane } = useAdminStore()

  useEffect(() => {
    void loadControlPlane()
    const interval = setInterval(() => {
      if (document.visibilityState === 'hidden') return
      void loadControlPlane()
    }, CONTROL_PLANE_REFRESH_MS)
    return () => clearInterval(interval)
  }, [loadControlPlane])

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Control-plane health</h2>
          <p className={uiStyles.sectionDescription}>
            Team-level agent coordination checks: refreshes when opened, then every 30 seconds while
            Admin is open. Any non-zero value below may need an owner to check stuck work.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadControlPlane()}
          disabled={controlPlaneLoading}
          className={uiStyles.secondaryButton}
        >
          {controlPlaneLoading ? 'Refreshing' : 'Refresh'}
        </button>
      </div>

      {/* Error */}
      {controlPlaneError && !controlPlane && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {controlPlaneErrorMessage(controlPlaneError)}
        </div>
      )}

      {/* Loading skeleton */}
      {controlPlaneLoading && !controlPlane && (
        <div className="flex items-center justify-center py-12">
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading Control Plane status
          </p>
        </div>
      )}

      {/* Content */}
      {controlPlane && (
        <>
          <div className={cn(uiStyles.card)}>
            {SIGNAL_DEFS.map((def) => (
              <SignalRow
                key={def.key}
                label={def.label}
                value={controlPlane[def.key]}
                warnNonZero={def.warnNonZero}
                unit={def.unit}
              />
            ))}
          </div>

          <p className="mt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Check-in rule: no agent update for {controlPlane.staleAfterSeconds}s counts as not
            checking in.
          </p>

          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Background task backlog is not shown in this team view. Owners can open{' '}
            <span className="font-mono">/metrics</span> when they need platform-wide numbers.
          </p>
        </>
      )}
    </div>
  )
}
