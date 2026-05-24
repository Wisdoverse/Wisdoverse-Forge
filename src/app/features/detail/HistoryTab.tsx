import { useEffect, useState } from 'react'
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  CircleDot,
  Clock3,
  Send,
  XCircle,
  type LucideIcon,
} from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import { cn } from '@app/shared/lib/utils'
import {
  orchestrationApi,
  taskResultArtifacts,
  type TaskRunSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'

interface HistoryTabProps {
  task: TaskSummary
}

export function HistoryTab({ task }: HistoryTabProps) {
  const events = taskHistoryEvents(task)
  const [runs, setRuns] = useState<TaskRunSummary[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    orchestrationApi
      .getTaskRuns(task.id)
      .then((items) => {
        if (!cancelled) setRuns(items)
      })
      .catch((err) => {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Failed to load task runs')
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [task.id])

  return (
    <div className="py-3" data-testid="task-updates">
      <div className="space-y-3">
        <AgentCheckIn task={task} />

        <section className="space-y-2">
          <p className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
            Lifecycle
          </p>
          {events.map((event) => (
            <div
              key={event.id}
              className="flex gap-2 rounded-lg bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]"
            >
              <span
                className="mt-1 h-2 w-2 shrink-0 rounded-full bg-apple-blue"
                aria-hidden="true"
              />
              <div className="min-w-0">
                <p className="text-xs font-medium text-foreground-light dark:text-foreground-dark">
                  {event.title}
                </p>
                <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
                  {event.detail}
                </p>
              </div>
            </div>
          ))}
        </section>

        <section className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
              Execution log
            </p>
            {loading && (
              <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
                Loading
              </span>
            )}
          </div>
          {error && (
            <div className="rounded-lg bg-apple-red/10 px-3 py-2 text-xs text-apple-red">
              {error}
            </div>
          )}
          {!loading && !error && runs.length === 0 && (
            <div className="rounded-lg border border-dashed border-black/[0.1] px-3 py-2 text-xs text-secondary-light dark:border-white/[0.12] dark:text-secondary-dark">
              Execution attempts appear here after the task is dispatched to an agent.
            </div>
          )}
          {runs.map((run) => (
            <TaskRunRow key={run.id} run={run} />
          ))}
        </section>
      </div>
    </div>
  )
}

function AgentCheckIn({ task }: { task: TaskSummary }) {
  const checkIn = taskCheckIn(task)
  const Icon = checkIn.Icon

  return (
    <section
      data-testid="task-agent-check-in"
      className={cn(
        'rounded-lg border p-3',
        checkIn.tone === 'warn'
          ? 'border-apple-orange/25 bg-apple-orange/[0.06]'
          : checkIn.tone === 'success'
            ? 'border-apple-green/20 bg-apple-green/[0.06]'
            : checkIn.tone === 'danger'
              ? 'border-apple-red/20 bg-apple-red/[0.06]'
              : 'border-black/[0.08] bg-white/70 dark:border-white/[0.1] dark:bg-white/[0.035]'
      )}
    >
      <div className="mb-3 flex items-start gap-2">
        <span
          className={cn(
            'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg',
            checkIn.tone === 'warn'
              ? 'bg-apple-orange/12 text-apple-orange'
              : checkIn.tone === 'success'
                ? 'bg-apple-green/12 text-apple-green'
                : checkIn.tone === 'danger'
                  ? 'bg-apple-red/12 text-apple-red'
                  : 'bg-apple-blue/10 text-apple-blue'
          )}
        >
          <Icon size={16} strokeWidth={2.2} aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
            Agent check-in
          </p>
          <p className="mt-0.5 text-xs font-semibold text-foreground-light dark:text-foreground-dark">
            {checkIn.title}
          </p>
          <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
            {checkIn.detail}
          </p>
        </div>
      </div>
      <div className="grid grid-cols-3 gap-2">
        <CheckInMetric label="Agent" value={task.assignedAgentName ?? 'Unassigned'} />
        <CheckInMetric label="State" value={stateLabel(task.state)} />
        <CheckInMetric label="Updated" value={formatRelativeTime(task.updatedAt)} />
      </div>
    </section>
  )
}

function CheckInMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-lg bg-black/[0.035] px-2 py-1.5 dark:bg-white/[0.045]">
      <p className="truncate text-[9px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
        {label}
      </p>
      <p className="mt-0.5 truncate text-[10px] font-semibold text-foreground-light dark:text-foreground-dark">
        {value}
      </p>
    </div>
  )
}

function TaskRunRow({ run }: { run: TaskRunSummary }) {
  const runtime =
    run.cliTool ??
    run.providerName ??
    run.runtimeKind ??
    (run.maxContextTokens ? 'runtime' : 'unknown runtime')
  const finished = run.finishedAt ? formatRelativeTime(run.finishedAt) : 'Still running'

  return (
    <div className="rounded-lg bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-xs font-medium text-foreground-light dark:text-foreground-dark">
            {run.status} run on {runtime}
          </p>
          <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
            Started {formatRelativeTime(run.startedAt)} · {finished}
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-black/[0.05] px-2 py-0.5 text-[10px] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          {run.id.slice(0, 8)}
        </span>
      </div>
    </div>
  )
}

function taskCheckIn(task: TaskSummary): {
  title: string
  detail: string
  tone: 'default' | 'success' | 'warn' | 'danger'
  Icon: LucideIcon
} {
  const agentName = task.assignedAgentName ?? 'The agent'
  const artifactCount = taskResultArtifacts(task.result).length

  switch (task.state) {
    case 'backlog':
      return task.assignedAgentName
        ? {
            title: `${agentName} is ready for dispatch`,
            detail: 'Queue the task when the runtime is ready to claim the work.',
            tone: 'default',
            Icon: Send,
          }
        : {
            title: 'No agent assigned yet',
            detail: 'Select an available agent before this task can leave the backlog.',
            tone: 'warn',
            Icon: Bot,
          }
    case 'queued':
      return {
        title: `${agentName} is waiting for a runtime slot`,
        detail: 'The task is queued and will move to active work when execution starts.',
        tone: 'default',
        Icon: Clock3,
      }
    case 'working':
      return {
        title: `${agentName} is working at ${task.progress}%`,
        detail:
          task.progress >= 80
            ? 'Prepare to review the handoff once artifacts arrive.'
            : 'Progress is active; watch for blockers or owner-input requests.',
        tone: 'default',
        Icon: CircleDot,
      }
    case 'blocked':
      return {
        title: `${agentName} needs owner input`,
        detail: task.blockedHint ?? task.blockedReason ?? 'Resolve the blocker to continue.',
        tone: 'warn',
        Icon: AlertTriangle,
      }
    case 'completed':
      return {
        title: `${agentName} completed the handoff`,
        detail:
          artifactCount > 0
            ? `${artifactCount} artifact${artifactCount === 1 ? '' : 's'} ready for review.`
            : 'Review the outcome and decide whether reusable learning should be drafted.',
        tone: 'success',
        Icon: CheckCircle2,
      }
    case 'failed':
      return {
        title: `${agentName} hit a run failure`,
        detail: task.error ?? 'Inspect the execution log before retrying this task.',
        tone: 'danger',
        Icon: XCircle,
      }
    case 'canceled':
      return {
        title: 'No active agent run',
        detail: 'The task was canceled; reopen or create follow-up work if needed.',
        tone: 'default',
        Icon: XCircle,
      }
  }
}

function stateLabel(state: TaskSummary['state']): string {
  switch (state) {
    case 'backlog':
      return 'Backlog'
    case 'queued':
      return 'Queued'
    case 'working':
      return 'Working'
    case 'blocked':
      return 'Blocked'
    case 'completed':
      return 'Completed'
    case 'failed':
      return 'Failed'
    case 'canceled':
      return 'Canceled'
  }
}

function taskHistoryEvents(task: TaskSummary): { id: string; title: string; detail: string }[]
function taskHistoryEvents(task: TaskSummary): { id: string; title: string; detail: string }[] {
  const events = [
    {
      id: 'created',
      title: 'Task created',
      detail: formatRelativeTime(task.createdAt),
    },
  ]

  if (task.assignedAgentName) {
    events.push({
      id: 'assigned',
      title: `Assigned to ${task.assignedAgentName}`,
      detail: task.assignedTo ? `Agent ${task.assignedTo.slice(0, 8)}` : 'Assignment recorded',
    })
  }

  if (task.state === 'working') {
    events.push({
      id: 'progress',
      title: `Work in progress at ${task.progress}%`,
      detail: `Updated ${formatRelativeTime(task.updatedAt)}`,
    })
  }

  if (task.state === 'blocked') {
    events.push({
      id: 'blocked',
      title: 'Task is blocked',
      detail: task.blockedHint ?? task.blockedReason ?? 'No blocker detail provided',
    })
  }

  if (task.state === 'failed') {
    events.push({
      id: 'failed',
      title: 'Run failed',
      detail: task.error ?? `Updated ${formatRelativeTime(task.updatedAt)}`,
    })
  }

  if (task.state === 'completed') {
    events.push({
      id: 'completed',
      title: 'Work completed',
      detail: task.completedAt
        ? formatRelativeTime(task.completedAt)
        : `Updated ${formatRelativeTime(task.updatedAt)}`,
    })
  }

  return events
}
