import { useEffect, useState } from 'react'
import { formatRelativeTime } from '@app/shared/lib/time'
import {
  orchestrationApi,
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
