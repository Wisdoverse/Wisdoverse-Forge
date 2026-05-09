import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { orchestrationApi, type TaskState, type TaskSummary } from '@app/shared/api/orchestration'

interface AgentTasksTabProps {
  agentId: string
}

const STATE_ORDER: TaskState[] = [
  'working',
  'queued',
  'backlog',
  'blocked',
  'completed',
  'failed',
  'canceled',
]

const STATE_LABELS: Record<TaskState, string> = {
  working: 'Working',
  queued: 'Queued',
  backlog: 'Backlog',
  blocked: 'Blocked',
  completed: 'Completed',
  failed: 'Failed',
  canceled: 'Canceled',
}

const STATE_DOT: Record<TaskState, string> = {
  working: 'bg-[#1d1d1f] dark:bg-white',
  queued: 'bg-[#7a7a7a]',
  backlog: 'bg-apple-gray-2',
  blocked: 'bg-apple-red',
  completed: 'bg-apple-gray-2',
  failed: 'bg-apple-red',
  canceled: 'bg-apple-gray-3',
}

export function AgentTasksTab({ agentId }: AgentTasksTabProps) {
  const [tasks, setTasks] = useState<TaskSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    orchestrationApi
      .getTasksByAgent(agentId, { limit: 100 })
      .then((list) => {
        if (!cancelled) setTasks(list)
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Failed to load tasks')
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [agentId])

  if (loading) {
    return (
      <div
        data-testid="agent-tasks-loading"
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
          'border border-black/[0.08] dark:border-white/[0.1]',
          'animate-pulse text-center text-ui-body text-secondary-light dark:text-secondary-dark'
        )}
      >
        Loading tasks…
      </div>
    )
  }

  if (error) {
    return (
      <div
        data-testid="agent-tasks-error"
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
          'border border-black/[0.08] dark:border-white/[0.1]',
          'text-center text-ui-body text-apple-red'
        )}
      >
        {error}
      </div>
    )
  }

  if (tasks.length === 0) {
    return (
      <div
        data-testid="agent-tasks-empty"
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
          'border border-black/[0.08] dark:border-white/[0.1]',
          'text-center text-ui-body text-secondary-light dark:text-secondary-dark'
        )}
      >
        No tasks have been routed to this agent yet.
      </div>
    )
  }

  // Group tasks by state for compact rendering. STATE_ORDER puts active work
  // (working/queued/backlog/blocked) above terminal states (completed/failed/canceled).
  const grouped: Partial<Record<TaskState, TaskSummary[]>> = {}
  for (const task of tasks) (grouped[task.state] ??= []).push(task)

  return (
    <div data-testid="agent-tasks" className="flex flex-col gap-4">
      {STATE_ORDER.map((state) => {
        const list = grouped[state]
        if (!list || list.length === 0) return null
        return (
          <section key={state} className="flex flex-col gap-2">
            <header className="flex items-center gap-2 px-1">
              <span className={cn('w-2 h-2 rounded-full', STATE_DOT[state])} />
              <h3 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                {STATE_LABELS[state]}
              </h3>
              <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {list.length}
              </span>
            </header>
            <ul className="flex flex-col gap-1.5">
              {list.map((task) => (
                <AgentTaskRow key={task.id} task={task} />
              ))}
            </ul>
          </section>
        )
      })}
    </div>
  )
}

function AgentTaskRow({ task }: { task: TaskSummary }) {
  const showProgress = task.state === 'working' && task.progress > 0
  return (
    <li
      data-testid={`agent-task-row-${task.id}`}
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-card px-3 py-2.5',
        'border border-black/[0.08] dark:border-white/[0.1]',
        'flex flex-col gap-1.5'
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <p className="line-clamp-2 flex-1 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {task.params.task || '(untitled)'}
        </p>
        <span className="shrink-0 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatRelativeTime(task.createdAt)}
        </span>
      </div>

      {showProgress && (
        <div className="h-1 bg-apple-gray-5 dark:bg-white/10 rounded-full overflow-hidden">
          <div
            data-testid={`agent-task-progress-${task.id}`}
            className="h-full rounded-full bg-apple-blue transition-all"
            style={{ width: `${task.progress}%` }}
          />
        </div>
      )}

      {task.state === 'blocked' && task.blockedHint && (
        <p
          data-testid={`agent-task-blocked-${task.id}`}
          className="flex items-start gap-1 text-ui-caption font-medium text-apple-red"
          title={task.blockedHint}
        >
          <span aria-hidden="true">⚠</span>
          <span className="line-clamp-2">{task.blockedHint}</span>
        </p>
      )}

      {task.state === 'failed' && task.error && (
        <p
          data-testid={`agent-task-error-${task.id}`}
          className="line-clamp-1 text-ui-caption text-apple-red"
          title={task.error}
        >
          {task.error}
        </p>
      )}
    </li>
  )
}
