import { useEffect, useMemo, useState } from 'react'
import {
  AlertTriangle,
  CheckCircle2,
  CircleDot,
  Clock3,
  ListFilter,
  Search,
  type LucideIcon,
} from 'lucide-react'
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

type AgentTaskFilter = 'all' | 'open' | 'needs-action' | 'completed'

const TASK_FILTERS: { value: AgentTaskFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'open', label: 'Open' },
  { value: 'needs-action', label: 'Needs action' },
  { value: 'completed', label: 'Completed' },
]

export function AgentTasksTab({ agentId }: AgentTasksTabProps) {
  const [tasks, setTasks] = useState<TaskSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [filter, setFilter] = useState<AgentTaskFilter>('all')
  const [query, setQuery] = useState('')

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

  const workload = useMemo(() => summarizeTasks(tasks), [tasks])
  const filterCounts = useMemo(
    () =>
      TASK_FILTERS.map((item) => ({
        ...item,
        count: countTasksForFilter(tasks, item.value),
      })),
    [tasks]
  )
  const visibleTasks = useMemo(
    () => tasks.filter((task) => taskMatchesFilter(task, filter, query)),
    [filter, query, tasks]
  )

  // Group tasks by state for compact rendering. STATE_ORDER puts active work
  // (working/queued/backlog/blocked) above terminal states (completed/failed/canceled).
  const grouped = useMemo(() => groupTasksByState(visibleTasks), [visibleTasks])

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

  return (
    <div data-testid="agent-tasks" className="flex flex-col gap-4">
      <section
        data-testid="agent-task-workload"
        className="rounded-xl border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]"
      >
        <div className="mb-3 flex items-start gap-2">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
            <ListFilter size={16} strokeWidth={2.15} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Work queue
            </p>
            <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Agent task load
            </h3>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <WorkloadMetric
            testId="agent-task-metric-active"
            label="Active"
            value={workload.active}
            Icon={CircleDot}
            tone="active"
          />
          <WorkloadMetric
            testId="agent-task-metric-backlog"
            label="Backlog"
            value={workload.backlog}
            Icon={Clock3}
            tone="neutral"
          />
          <WorkloadMetric
            testId="agent-task-metric-needs-action"
            label="Needs action"
            value={workload.needsAction}
            Icon={AlertTriangle}
            tone="warn"
          />
          <WorkloadMetric
            testId="agent-task-metric-completed"
            label="Completed"
            value={workload.completed}
            Icon={CheckCircle2}
            tone="success"
          />
        </div>
      </section>

      <div className="flex flex-col gap-2">
        <label className="relative block">
          <Search
            size={14}
            strokeWidth={2}
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
            aria-hidden="true"
          />
          <input
            data-testid="agent-task-search"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search tasks, blockers, results..."
            className={cn(
              'h-9 w-full rounded-lg border border-black/[0.08] bg-white pl-8 pr-3 text-ui-body outline-none',
              'text-foreground-light placeholder:text-secondary-light dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-foreground-dark dark:placeholder:text-secondary-dark',
              'focus:ring-2 focus:ring-apple-blue-focus'
            )}
          />
        </label>
        <div
          role="group"
          aria-label="Agent task filter"
          data-testid="agent-task-filter-group"
          className="inline-flex max-w-full items-center gap-1 overflow-x-auto rounded-lg bg-black/[0.035] p-1 dark:bg-white/[0.05]"
        >
          {filterCounts.map((item) => (
            <TaskFilterButton
              key={item.value}
              active={filter === item.value}
              label={item.label}
              count={item.count}
              onClick={() => setFilter(item.value)}
            />
          ))}
        </div>
      </div>

      {visibleTasks.length > 0 ? (
        STATE_ORDER.map((state) => {
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
        })
      ) : (
        <div
          data-testid="agent-tasks-filter-empty"
          className={cn(
            'rounded-xl border border-dashed border-black/[0.1] bg-white px-4 py-6',
            'text-center text-ui-body text-secondary-light dark:border-white/[0.12] dark:bg-[#2c2c2e] dark:text-secondary-dark'
          )}
        >
          No tasks match this view.
        </div>
      )}
    </div>
  )
}

function WorkloadMetric({
  testId,
  label,
  value,
  Icon,
  tone,
}: {
  testId: string
  label: string
  value: number
  Icon: LucideIcon
  tone: 'active' | 'neutral' | 'warn' | 'success'
}) {
  return (
    <div
      data-testid={testId}
      className="min-w-0 rounded-lg bg-black/[0.03] px-3 py-2 dark:bg-white/[0.04]"
    >
      <div className="mb-1 flex items-center gap-1.5 text-secondary-light dark:text-secondary-dark">
        <Icon
          size={12}
          strokeWidth={2.2}
          className={cn({
            'text-apple-blue': tone === 'active' || tone === 'neutral',
            'text-apple-orange': tone === 'warn',
            'text-apple-green': tone === 'success',
          })}
          aria-hidden="true"
        />
        <span className="truncate text-[10px] font-medium">{label}</span>
      </div>
      <p className="text-ui-section font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
        {value}
      </p>
    </div>
  )
}

function TaskFilterButton({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean
  label: string
  count: number
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        'inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-ui-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
        active
          ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
          : 'text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark'
      )}
    >
      <span>{label}</span>
      <span className="tabular-nums text-secondary-light dark:text-secondary-dark">{count}</span>
    </button>
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
          <AlertTriangle size={12} strokeWidth={2.25} className="mt-0.5 shrink-0" />
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

function summarizeTasks(tasks: TaskSummary[]): {
  active: number
  backlog: number
  needsAction: number
  completed: number
} {
  return tasks.reduce(
    (summary, task) => {
      if (task.state === 'working' || task.state === 'queued') summary.active += 1
      if (task.state === 'backlog') summary.backlog += 1
      if (task.state === 'blocked' || task.state === 'failed') summary.needsAction += 1
      if (task.state === 'completed') summary.completed += 1
      return summary
    },
    { active: 0, backlog: 0, needsAction: 0, completed: 0 }
  )
}

function countTasksForFilter(tasks: TaskSummary[], filter: AgentTaskFilter): number {
  if (filter === 'all') return tasks.length
  return tasks.filter((task) => taskMatchesFilter(task, filter, '')).length
}

function taskMatchesFilter(task: TaskSummary, filter: AgentTaskFilter, query: string): boolean {
  const matchesFilter =
    filter === 'all' ||
    (filter === 'open' &&
      (task.state === 'working' || task.state === 'queued' || task.state === 'backlog')) ||
    (filter === 'needs-action' && (task.state === 'blocked' || task.state === 'failed')) ||
    (filter === 'completed' && task.state === 'completed')
  if (!matchesFilter) return false

  const normalizedQuery = query.trim().toLowerCase()
  if (!normalizedQuery) return true

  return [
    task.params.task,
    task.params.message,
    task.blockedHint,
    task.blockedReason,
    task.error,
  ].some((value) => value?.toLowerCase().includes(normalizedQuery))
}

function groupTasksByState(tasks: TaskSummary[]): Partial<Record<TaskState, TaskSummary[]>> {
  return tasks.reduce<Partial<Record<TaskState, TaskSummary[]>>>((grouped, task) => {
    const stateTasks = grouped[task.state] ?? []
    stateTasks.push(task)
    grouped[task.state] = stateTasks
    return grouped
  }, {})
}
