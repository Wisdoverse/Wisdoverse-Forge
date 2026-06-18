import { RefreshCw, UserCheck } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { agentCapabilitySummary } from '@app/shared/lib/agentCapabilityCopy'
import type { ParticipantSummary } from '@app/shared/api/orchestration'

export interface BoardWorkloadSnapshot {
  backlog: number
  unassigned: number
  inFlight: number
  blocked: number
  review: number
}

interface AssignmentReadinessPanelProps {
  participants: ParticipantSummary[]
  workload: BoardWorkloadSnapshot
  loading: boolean
  error: string | null
  onRefresh: () => void
}

const STATUS_LABELS: Record<ParticipantSummary['status'], string> = {
  available: 'Can take work',
  busy: 'Working now',
  offline: 'Not connected',
}

const STATUS_STYLES: Record<ParticipantSummary['status'], string> = {
  available: 'bg-apple-green text-white',
  busy: 'bg-apple-orange text-white',
  offline: 'bg-apple-gray-2 text-white',
}

export function AssignmentReadinessPanel({
  participants,
  workload,
  loading,
  error,
  onRefresh,
}: AssignmentReadinessPanelProps) {
  const available = participants.filter((participant) => participant.status === 'available')
  const busy = participants.filter((participant) => participant.status === 'busy')
  const offline = participants.filter((participant) => participant.status === 'offline')
  const summary =
    participants.length === 0
      ? 'Connect an agent before sending work.'
      : available.length > 0
        ? `${available.length} agent${available.length === 1 ? '' : 's'} can take work now.`
        : 'Open Agents to start or connect an agent, or wait for one to finish.'
  const handoffSummary = summarizeHandoff(workload, available.length)

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
                Agent status
              </h2>
              <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {loading ? 'Checking agent status…' : summary}
              </span>
            </div>
            <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {handoffSummary}
            </p>
            {error && <p className="mt-0.5 text-ui-caption text-apple-red">{error}</p>}
          </div>
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <MetricPill label="Can take work" value={available.length} />
          <MetricPill label="Working now" value={busy.length} />
          <MetricPill label="Not connected" value={offline.length} />
          <button
            type="button"
            onClick={onRefresh}
            disabled={loading}
            className="flex h-8 w-8 items-center justify-center rounded-lg text-secondary-light transition-colors hover:bg-black/[0.05] hover:text-foreground-light disabled:opacity-50 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
            aria-label="Refresh agent status"
            title="Refresh agent status"
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

      <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-black/[0.06] pt-2 dark:border-white/[0.08]">
        <MetricPill
          label="Not sent yet"
          value={workload.backlog}
          testId="assignment-metric-backlog"
        />
        <MetricPill
          label="Needs agent"
          value={workload.unassigned}
          testId="assignment-metric-unassigned"
        />
        <MetricPill
          label="Being worked on"
          value={workload.inFlight}
          testId="assignment-metric-working"
        />
        <MetricPill
          label="Needs help"
          value={workload.blocked}
          tone={workload.blocked > 0 ? 'warn' : 'default'}
          testId="assignment-metric-blocked"
        />
        <MetricPill
          label="Review"
          value={workload.review}
          tone={workload.review > 0 ? 'success' : 'default'}
        />
      </div>

      {participants.length === 0 && !loading ? (
        <div
          data-testid="assignment-readiness-empty"
          className="mt-2 rounded-lg border border-dashed border-apple-blue/25 bg-apple-blue/[0.04] px-3 py-2"
        >
          <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
            Connect an agent before sending work
          </p>
          <p className="mt-0.5 text-ui-caption leading-snug text-secondary-light dark:text-secondary-dark">
            Set up where tasks wait, choose that place for this project, and add an available agent.
            Until then, tasks that are not sent yet will wait here.
          </p>
        </div>
      ) : participants.length > 0 ? (
        <div className="mt-2 flex gap-2 overflow-x-auto pb-0.5">
          {participants.map((participant) => (
            <ParticipantChip key={participant.agentId} participant={participant} />
          ))}
        </div>
      ) : null}
    </section>
  )
}

function MetricPill({
  label,
  value,
  tone = 'default',
  testId,
}: {
  label: string
  value: number
  tone?: 'default' | 'success' | 'warn'
  testId?: string
}) {
  return (
    <span
      data-testid={testId ?? `assignment-metric-${label.toLowerCase().replace(/\s+/g, '-')}`}
      className={cn(
        'inline-flex h-7 items-center gap-1.5 rounded-full px-2 text-ui-caption',
        tone === 'success'
          ? 'bg-apple-green/10 text-apple-green'
          : tone === 'warn'
            ? 'bg-apple-orange/10 text-apple-orange'
            : 'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
      )}
    >
      <span className="font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
        {value}
      </span>
      {label}
    </span>
  )
}

function summarizeHandoff(workload: BoardWorkloadSnapshot, availableCount: number): string {
  if (workload.unassigned > 0) {
    const taskLabel = pluralize(workload.unassigned, 'task')
    const verb = workload.unassigned === 1 ? 'needs' : 'need'
    const pronoun = workload.unassigned === 1 ? 'it' : 'them'

    return availableCount > 0
      ? `${workload.unassigned} ${taskLabel} ${verb} an agent. Choose an available agent to start ${pronoun}.`
      : `${workload.unassigned} ${taskLabel} ${verb} an agent. Open Agents to start or connect an agent, or wait for one to finish.`
  }

  if (workload.blocked > 0) {
    const taskLabel = pluralize(workload.blocked, 'task')
    const verb = workload.blocked === 1 ? 'needs' : 'need'
    const pronoun = workload.blocked === 1 ? 'it' : 'they'

    return `${workload.blocked} ${taskLabel} ${verb} help before ${pronoun} can continue.`
  }

  if (workload.review > 0) {
    return `${workload.review} completed ${pluralize(workload.review, 'task')} ready for review.`
  }

  return 'Create a task when you have work to send.'
}

function pluralize(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`
}

function ParticipantChip({ participant }: { participant: ParticipantSummary }) {
  const reason =
    participant.status === 'available'
      ? 'Can take a task now'
      : participant.status === 'busy'
        ? 'Already working'
        : participant.lastHeartbeatAt
          ? `Last seen ${formatRelativeTime(participant.lastHeartbeatAt)}`
          : 'Open Agents to reconnect'
  const capabilities =
    participant.capabilities.length > 0 ? agentCapabilitySummary(participant.capabilities) : ''
  const detail =
    participant.status === 'available' ? capabilities || reason : joinDetails(reason, capabilities)

  return (
    <div className="flex min-w-[180px] items-center justify-between gap-2 rounded-lg bg-black/[0.03] px-2.5 py-2 dark:bg-white/[0.04]">
      <div className="min-w-0">
        <p className="truncate text-ui-button font-medium text-foreground-light dark:text-foreground-dark">
          {participant.name}
        </p>
        <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {detail}
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

function joinDetails(...parts: Array<string | undefined>): string {
  return parts.filter(Boolean).join(' · ')
}
