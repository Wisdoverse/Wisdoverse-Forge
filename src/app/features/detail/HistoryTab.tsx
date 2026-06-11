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
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { taskStateLabel } from '@app/entities/task'
import {
  orchestrationApi,
  taskResultArtifacts,
  type TaskRunSummary,
  type TaskSummary,
} from '@app/shared/api/orchestration'
import { taskDetailErrorMessage } from './taskDetailErrorMessages'

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
        if (!cancelled) setError(taskDetailErrorMessage('loadRuns', err))
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

        <section
          data-testid="task-updates-guide"
          className="rounded-lg border border-apple-blue/15 bg-apple-blue/[0.055] px-3 py-2.5 dark:border-apple-blue/25 dark:bg-apple-blue/[0.09]"
        >
          <p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-apple-blue">
            What to check now
          </p>
          <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
            {taskUpdateGuide(task)}
          </p>
        </section>

        <section className="space-y-2">
          <p className="text-[10px] font-medium uppercase text-secondary-light dark:text-secondary-dark">
            Task story
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
              Agent work history
            </p>
            {loading && (
              <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
                Loading work history
              </span>
            )}
          </div>
          {error && (
            <div
              role="alert"
              className="rounded-lg bg-apple-red/10 px-3 py-2 text-xs text-apple-red"
            >
              {error}
            </div>
          )}
          {!loading && !error && runs.length === 0 && (
            <div className="rounded-lg border border-dashed border-black/[0.1] px-3 py-2 text-xs text-secondary-light dark:border-white/[0.12] dark:text-secondary-dark">
              Work history appears after an agent starts. If this stays empty, check that an agent
              is assigned and the task has been started.
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
            Current status
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
        <CheckInMetric label="State" value={taskStateLabel(task.state)} />
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
  const runSource = runSourceLabel(run)
  const finished = run.finishedAt ? formatRelativeTime(run.finishedAt) : 'Still running'
  const supportReference = run.id.slice(0, 8)

  return (
    <div className="rounded-lg bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-xs font-medium text-foreground-light dark:text-foreground-dark">
            Work attempt: {readableRunStatus(run.status)}
          </p>
          <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
            Started {formatRelativeTime(run.startedAt)} · {finished} · Used {runSource}
          </p>
        </div>
        <span className="shrink-0 rounded-full bg-black/[0.05] px-2 py-0.5 text-[10px] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          Support reference {supportReference}
        </span>
      </div>
    </div>
  )
}

function runSourceLabel(run: TaskRunSummary): string {
  const cliTool = workToolLabel(run.cliTool)
  if (cliTool) return cliTool
  const provider = aiServiceLabel(run.providerName)
  if (provider) return provider

  switch (run.runtimeKind) {
    case 'container':
      return 'a managed workspace'
    case 'cli':
    case 'host':
      return 'this computer'
    case 'api':
    case 'provider':
      return 'an AI service'
    default:
      return run.maxContextTokens ? 'the assigned agent' : 'an agent'
  }
}

function aiServiceLabel(providerName?: string): string | null {
  const trimmed = providerName?.trim()
  if (!trimmed) return null

  const normalized = trimmed.toLowerCase()
  switch (normalized) {
    case 'anthropic':
      return 'Anthropic'
    case 'openai':
      return 'OpenAI'
    case 'google':
    case 'gemini':
      return 'Google'
    case 'openai_compatible':
    case 'openai-compatible':
    case 'custom':
      return 'a custom AI service'
    case 'azure_openai':
    case 'azure-openai':
      return 'Azure OpenAI'
    case 'ollama':
    case 'local':
      return 'a local AI service'
    default:
      return looksLikeSlug(trimmed, normalized) ? 'an AI service that needs review' : trimmed
  }
}

function looksLikeSlug(value: string, normalized: string): boolean {
  return value === normalized && /^[a-z0-9]+(?:[_-][a-z0-9]+)+$/.test(normalized)
}

function workToolLabel(tool?: string): string | null {
  switch (tool?.trim().toLowerCase()) {
    case 'claude':
      return 'Claude'
    case 'codex':
      return 'Codex'
    case 'gemini':
      return 'Gemini'
    case 'opencode':
      return 'OpenCode'
    case undefined:
    case '':
      return null
    default:
      return 'a work tool that needs review'
  }
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
            title: `${agentName} is ready to start`,
            detail: 'Start the task when you are ready for the agent to begin.',
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
        title: `${agentName} is waiting to start`,
        detail: 'Nothing is needed yet. The task will move to active work when an agent begins.',
        tone: 'default',
        Icon: Clock3,
      }
    case 'working':
      return {
        title: `${agentName} is working at ${task.progress}%`,
        detail:
          task.progress >= 80
            ? 'Prepare to review the result when the task finishes.'
            : 'Progress is active. Watch for requests that need your decision.',
        tone: 'default',
        Icon: CircleDot,
      }
    case 'blocked':
      return {
        title: `${agentName} needs owner input`,
        detail: taskBlockedPreview({
          blockedHint: task.blockedHint,
          blockedReason: task.blockedReason,
          error: task.error,
        }),
        tone: 'warn',
        Icon: AlertTriangle,
      }
    case 'completed':
      return {
        title: `${agentName} finished the task`,
        detail:
          artifactCount > 0
            ? `${artifactCount} result item${artifactCount === 1 ? '' : 's'} ready to review.`
            : 'Review the outcome and decide whether reusable learning should be drafted.',
        tone: 'success',
        Icon: CheckCircle2,
      }
    case 'failed':
      return {
        title: `${agentName} could not finish`,
        detail: taskFailurePreview(task.error),
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
    default:
      return {
        title: 'Task status needs review',
        detail:
          'Open the latest updates before deciding whether to start, retry, or close this task.',
        tone: 'warn',
        Icon: AlertTriangle,
      }
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
      detail: 'This agent is responsible for the next step.',
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
      title: 'Needs your input',
      detail: taskBlockedPreview({
        blockedHint: task.blockedHint,
        blockedReason: task.blockedReason,
        error: task.error,
      }),
    })
  }

  if (task.state === 'failed') {
    events.push({
      id: 'failed',
      title: 'Work stopped',
      detail: taskFailurePreview(task.error),
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

function taskUpdateGuide(task: TaskSummary): string {
  switch (task.state) {
    case 'backlog':
      return task.assignedAgentName
        ? 'The task has an agent. Start it when the brief is ready.'
        : 'Choose an agent first, then start the task.'
    case 'queued':
      return 'The task is waiting to begin. Check back if it stays here longer than expected.'
    case 'working':
      return 'The agent is working. Watch for requests that need your decision, then review the result when it finishes.'
    case 'blocked':
      return 'The task needs your input. Read the reason, decide what to provide, then approve or update the task.'
    case 'completed':
      return 'Open Results next. Confirm the answer matches the brief before reusing the work.'
    case 'failed':
      return 'Read the latest attempt, fix the cause if you can, then retry or create a clearer follow-up task.'
    case 'canceled':
      return 'No one is working on this task now. Reopen it or create follow-up work if it still matters.'
    default:
      return 'Review the latest updates before deciding whether to start, retry, or close this task.'
  }
}

function readableRunStatus(status: string): string {
  const normalized = normalizeRunStatus(status)
  switch (normalized) {
    case 'completed':
    case 'succeeded':
    case 'success':
      return 'Finished'
    case 'running':
    case 'working':
    case 'in_progress':
      return 'In progress'
    case 'queued':
    case 'pending':
      return 'Waiting to start'
    case 'failed':
    case 'error':
      return 'Needs review'
    case 'canceled':
    case 'cancelled':
      return 'Stopped'
    default:
      return normalized ? 'Status needs review' : 'Status not reported'
  }
}

function normalizeRunStatus(status: string): string {
  return status.trim().toLowerCase()
}
