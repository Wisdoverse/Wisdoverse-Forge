import { ArrowRight, RefreshCw, UserCheck } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { agentCapabilitySummary } from '@app/shared/lib/agentCapabilityCopy'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { ParticipantSummary } from '@app/shared/api/orchestration'
import { agentCanTakeTask, agentHasTaskCapability } from './model/agentTaskReadiness'

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

const STATUS_DOTS: Record<ParticipantSummary['status'], string> = {
  available: 'bg-apple-green',
  busy: 'bg-apple-orange',
  offline: 'bg-apple-gray-2',
}

export function AssignmentReadinessPanel({
  participants,
  workload,
  loading,
  error,
  onRefresh,
}: AssignmentReadinessPanelProps) {
  // Defensive: never assume a non-null list here, so a malformed/partial
  // readiness payload degrades to an empty panel instead of crashing the board.
  const roster = participants ?? []
  const taskCapable = roster.filter(agentHasTaskCapability)
  const available = roster.filter(agentCanTakeTask)
  const busy = taskCapable.filter((participant) => participant.status === 'busy')
  const offline = taskCapable.filter((participant) => participant.status === 'offline')
  const chatOnly = roster.filter((participant) => !agentHasTaskCapability(participant))
  const needsAgentSetup = roster.length === 0 || (available.length === 0 && workload.unassigned > 0)
  const summary =
    roster.length === 0
      ? 'Connect an agent before sending work.'
      : available.length > 0
        ? `${available.length} agent${available.length === 1 ? '' : 's'} can take work now.`
        : chatOnly.length === roster.length
          ? 'Simple chat agents answer in Chat. For Tasks, add an agent with Project files or This computer.'
          : 'Open Agents to start or connect an agent, or wait for one to finish.'
  const handoffSummary = summarizeHandoff(workload, available.length)
  const isCompactHealthy =
    !loading &&
    !error &&
    roster.length > 0 &&
    chatOnly.length === 0 &&
    available.length === taskCapable.length &&
    workload.backlog === 0 &&
    workload.unassigned === 0 &&
    workload.blocked === 0 &&
    workload.review === 0

  return (
    <section data-testid="assignment-readiness" className={cn(uiStyles.card, 'px-3 py-2')}>
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
            {error && (
              <p role="alert" aria-live="polite" className="mt-0.5 text-ui-caption text-apple-red">
                {error}
              </p>
            )}
          </div>
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          {needsAgentSetup && (
            <a href="/agents" className={uiStyles.primaryButton}>
              <span>Open Agents</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </a>
          )}
          <MetricPill label="Can take work" value={available.length} />
          <MetricPill label="Working now" value={busy.length} />
          <MetricPill label="Not connected" value={offline.length} />
          {chatOnly.length > 0 ? (
            <MetricPill label="Questions only" value={chatOnly.length} />
          ) : null}
          <button
            type="button"
            onClick={onRefresh}
            disabled={loading}
            className={cn(uiStyles.subtleButton, 'w-8 px-0')}
            aria-label="Check agent status"
            title="Check agent status"
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

      {!isCompactHealthy ? (
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
            label="Ready to check"
            value={workload.review}
            tone={workload.review > 0 ? 'success' : 'default'}
          />
        </div>
      ) : null}

      {!isCompactHealthy && roster.length === 0 && !loading ? (
        <div
          data-testid="assignment-readiness-empty"
          className="mt-2 rounded-card border border-dashed border-apple-blue/25 bg-apple-blue/[0.04] px-3 py-2"
        >
          <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
            Connect an agent before sending work
          </p>
          <p className="mt-0.5 text-ui-caption leading-snug text-secondary-light dark:text-secondary-dark">
            Set up a place for new tasks in this project, then add or start an agent. Until then,
            new tasks wait on this board.
          </p>
        </div>
      ) : !isCompactHealthy && roster.length > 0 ? (
        <div className="mt-2 flex gap-2 overflow-x-auto pb-0.5">
          {roster.map((participant) => (
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
  const toneDot = tone === 'success' ? 'bg-apple-green' : tone === 'warn' ? 'bg-apple-orange' : null

  return (
    <span
      data-testid={testId ?? `assignment-metric-${label.toLowerCase().replace(/\s+/g, '-')}`}
      className="inline-flex h-7 items-center gap-1.5 rounded-full bg-black/[0.04] px-2 text-ui-caption text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark"
    >
      {toneDot ? (
        <span className={cn('h-1.5 w-1.5 rounded-full', toneDot)} aria-hidden="true" />
      ) : null}
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
    return `${workload.review} completed ${pluralize(workload.review, 'task')} ready to check.`
  }

  if (workload.inFlight > 0) {
    const verb = workload.inFlight === 1 ? 'is' : 'are'
    return `${workload.inFlight} ${pluralize(workload.inFlight, 'task')} ${verb} being worked on.`
  }

  return 'Create a task when you have work to send.'
}

function pluralize(count: number, singular: string): string {
  return count === 1 ? singular : `${singular}s`
}

function ParticipantChip({ participant }: { participant: ParticipantSummary }) {
  const taskCapable = agentHasTaskCapability(participant)
  const reason = !taskCapable
    ? 'Simple chat only. Use an agent with Project files or This computer for Tasks.'
    : participant.status === 'available'
      ? 'Can take a task now'
      : participant.status === 'busy'
        ? 'Already working'
        : participant.lastHeartbeatAt
          ? `Last seen ${formatRelativeTime(participant.lastHeartbeatAt)}`
          : 'Open Agents to reconnect'
  const capabilities =
    taskCapable && participant.capabilities.length > 0
      ? agentCapabilitySummary(participant.capabilities)
      : ''
  const detail =
    !taskCapable || participant.status === 'available'
      ? capabilities || reason
      : joinDetails(reason, capabilities)
  const statusLabel = taskCapable ? STATUS_LABELS[participant.status] : 'Questions only'
  const statusDotClassName = taskCapable ? STATUS_DOTS[participant.status] : 'bg-apple-gray-2'

  return (
    <div className="flex min-w-[180px] items-center justify-between gap-2 rounded-card bg-black/[0.03] px-2.5 py-2 dark:bg-white/[0.04]">
      <div className="min-w-0">
        <p className="truncate text-ui-button font-medium text-foreground-light dark:text-foreground-dark">
          {participant.name}
        </p>
        <p className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {detail}
        </p>
      </div>
      <span
        className="inline-flex shrink-0 items-center gap-1.5 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
        title={reason}
      >
        <span className={cn('h-1.5 w-1.5 rounded-full', statusDotClassName)} aria-hidden="true" />
        {statusLabel}
      </span>
    </div>
  )
}

function joinDetails(...parts: Array<string | undefined>): string {
  return parts.filter(Boolean).join(' · ')
}
