import { RefreshCw, UserCheck } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { ParticipantSummary } from '@app/shared/api/orchestration'

interface AssignmentReadinessPanelProps {
  participants: ParticipantSummary[]
  loading: boolean
  error: string | null
  onRefresh: () => void
}

const STATUS_LABELS: Record<ParticipantSummary['status'], string> = {
  available: 'Available',
  busy: 'Busy',
  offline: 'Offline',
}

const STATUS_STYLES: Record<ParticipantSummary['status'], string> = {
  available: 'bg-apple-green text-white',
  busy: 'bg-apple-orange text-white',
  offline: 'bg-apple-gray-2 text-white',
}

export function AssignmentReadinessPanel({
  participants,
  loading,
  error,
  onRefresh,
}: AssignmentReadinessPanelProps) {
  const available = participants.filter((participant) => participant.status === 'available')
  const busy = participants.filter((participant) => participant.status === 'busy')
  const offline = participants.filter((participant) => participant.status === 'offline')
  const summary =
    participants.length === 0
      ? 'No agents are registered for this task group.'
      : available.length > 0
        ? `${available.length} agent${available.length === 1 ? '' : 's'} can take work now.`
        : 'No agent can take work right now.'

  return (
    <section
      data-testid="assignment-readiness"
      className="rounded-lg border border-black/[0.08] bg-white px-3 py-2 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
        <div className="flex min-w-0 items-start gap-2">
          <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
            <UserCheck size={16} strokeWidth={2.25} aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
              <h2 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                Assignment readiness
              </h2>
              <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {loading ? 'Checking agents...' : summary}
              </span>
            </div>
            {error && <p className="mt-0.5 text-ui-caption text-apple-red">{error}</p>}
          </div>
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <ReadinessCount label="Available" value={available.length} />
          <ReadinessCount label="Busy" value={busy.length} />
          <ReadinessCount label="Offline" value={offline.length} />
          <button
            type="button"
            onClick={onRefresh}
            disabled={loading}
            className="flex h-8 w-8 items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-black/[0.05] hover:text-foreground-light disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
            aria-label="Refresh assignment readiness"
            title="Refresh assignment readiness"
          >
            <RefreshCw
              size={14}
              strokeWidth={2}
              className={cn(loading && 'animate-spin')}
              aria-hidden="true"
            />
          </button>
        </div>
      </div>

      {participants.length > 0 && (
        <div className="mt-2 flex gap-2 overflow-x-auto pb-0.5">
          {participants.map((participant) => (
            <ParticipantChip key={participant.agentId} participant={participant} />
          ))}
        </div>
      )}
    </section>
  )
}

function ReadinessCount({ label, value }: { label: string; value: number }) {
  return (
    <span className="inline-flex h-7 items-center gap-1.5 rounded-full bg-black/[0.04] px-2 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
      <span className="font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
      {label}
    </span>
  )
}

function ParticipantChip({ participant }: { participant: ParticipantSummary }) {
  const reason =
    participant.status === 'available'
      ? 'Can take a task now'
      : participant.status === 'busy'
        ? 'Already working'
        : participant.lastHeartbeatAt
          ? `Last heartbeat ${formatRelativeTime(participant.lastHeartbeatAt)}`
          : 'No recent heartbeat'

  return (
    <div className="flex min-w-[180px] items-center justify-between gap-2 rounded-lg bg-black/[0.03] px-2.5 py-2 dark:bg-white/[0.04]">
      <div className="min-w-0">
        <p className="truncate text-ui-button font-medium text-foreground-light dark:text-foreground-dark">
          {participant.name}
        </p>
        <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {participant.capabilities.length > 0 ? participant.capabilities.join(', ') : reason}
        </p>
      </div>
      <span
        className={cn(
          'shrink-0 rounded-full px-2 py-0.5 text-ui-caption font-medium',
          STATUS_STYLES[participant.status]
        )}
        title={reason}
      >
        {STATUS_LABELS[participant.status]}
      </span>
    </div>
  )
}
