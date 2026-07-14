import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type OrgControlPlaneSnapshot } from '@app/entities/admin'
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
          isWarning ? `${label}: ${valueText}, check this value` : `${label}: ${valueText}`
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
    label: 'Work updates waiting to send',
    warnNonZero: true,
  },
  {
    key: 'assignmentOutboxOldestAgeSeconds',
    label: 'Oldest work update waiting (s)',
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
          <h2 className={uiStyles.sectionTitle}>Agent work checks</h2>
          <p className={uiStyles.sectionDescription}>
            Shows whether agents are getting work, checking in, and finishing tasks. Forge checks
            this when Admin opens, then every 30 seconds. If any number below is above 0, an owner
            may need to check stuck work.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadControlPlane()}
          disabled={controlPlaneLoading}
          className={uiStyles.secondaryButton}
        >
          {controlPlaneLoading ? 'Checking' : 'Check again'}
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
            Checking agent work
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
            If an agent sends no update for {controlPlane.staleAfterSeconds}s, it appears as not
            checking in.
          </p>

          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Background tasks waiting across the platform are not shown in this team view. Owners can
            check platform-wide numbers in their monitoring tools.
          </p>
        </>
      )}
    </div>
  )
}
