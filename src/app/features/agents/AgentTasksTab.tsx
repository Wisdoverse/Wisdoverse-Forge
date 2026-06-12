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
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { orchestrationApi, type TaskState, type TaskSummary } from '@app/shared/api/orchestration'
import { agentTasksErrorMessage } from './model/taskErrorMessage'

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
  working: 'Doing now',
  queued: 'Waiting to start',
  backlog: 'Ready for later',
  blocked: 'Needs your help',
  completed: 'Done',
  failed: 'Stopped with an error',
  canceled: 'Canceled',
}

const STATE_HELP: Record<TaskState, string> = {
  working: 'The agent is actively working on these tasks.',
  queued: 'These tasks are next in line for this agent.',
  backlog: 'These tasks are assigned but not started yet.',
  blocked: 'These tasks need a person to unblock them.',
  completed: 'These tasks are finished.',
  failed: 'These tasks stopped before finishing.',
  canceled: 'These tasks were stopped on purpose.',
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
  { value: 'open', label: 'Still open' },
  { value: 'needs-action', label: 'Needs help' },
  { value: 'completed', label: 'Done' },
]

const AGENT_TASK_EMPTY_STEPS: { title: string; description: string; Icon: LucideIcon }[] = [
  {
    title: 'Open Tasks',
    description: 'Create work, then choose this agent or a task queue it can receive.',
    Icon: ListFilter,
  },
  {
    title: 'Check the task queue',
    description: 'Open Agents, then Task Queues, and make sure this agent is attached.',
    Icon: CircleDot,
  },
  {
    title: 'Use Needs help after tasks arrive',
    description: 'Blocked or failed work appears there first, so you know what to fix.',
    Icon: AlertTriangle,
  },
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
        if (!cancelled) setError(agentTasksErrorMessage(err))
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
  const resetTaskFilters = () => {
    setFilter('all')
    setQuery('')
  }

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
        Checking this agent's work list...
      </div>
    )
  }

  if (error) {
    return (
      <div
        role="alert"
        data-testid="agent-tasks-error"
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
          'border border-black/[0.08] dark:border-white/[0.1]',
          'text-center text-ui-body text-apple-red'
        )}
      >
        <p className="font-medium">This agent's work list needs attention.</p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {error}
        </p>
      </div>
    )
  }

  if (tasks.length === 0) {
    return <AgentTasksEmptyState />
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
              Agent work list
            </p>
            <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              What this agent is handling
            </h3>
            <p className="mt-1 max-w-2xl text-ui-caption text-secondary-light dark:text-secondary-dark">
              Start with Needs help, then check Doing now and Waiting to start.
            </p>
          </div>
        </div>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <WorkloadMetric
            testId="agent-task-metric-active"
            label="Doing now"
            value={workload.active}
            Icon={CircleDot}
            tone="active"
          />
          <WorkloadMetric
            testId="agent-task-metric-backlog"
            label="Waiting"
            value={workload.backlog}
            Icon={Clock3}
            tone="neutral"
          />
          <WorkloadMetric
            testId="agent-task-metric-needs-action"
            label="Needs help"
            value={workload.needsAction}
            Icon={AlertTriangle}
            tone="warn"
          />
          <WorkloadMetric
            testId="agent-task-metric-completed"
            label="Done"
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
            placeholder="Search by task name, blocker, or result"
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
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <h3 className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                      {STATE_LABELS[state]}
                    </h3>
                    <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                      {list.length}
                    </span>
                  </div>
                  <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {STATE_HELP[state]}
                  </p>
                </div>
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
        <AgentTasksFilterEmptyState onReset={resetTaskFilters} />
      )}
    </div>
  )
}

function AgentTasksEmptyState() {
  return (
    <section
      data-testid="agent-tasks-empty"
      className={cn(
        'rounded-xl border border-black/[0.08] bg-white px-4 py-5 dark:border-white/[0.1] dark:bg-[#2c2c2e]',
        'text-foreground-light dark:text-foreground-dark'
      )}
    >
      <div className="mb-4 flex items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
          <ListFilter size={17} strokeWidth={2.15} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h3 className="text-ui-section font-semibold">No tasks have reached this agent yet</h3>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            Tasks appear here after work is sent to this agent or to a task queue it can receive.
          </p>
        </div>
      </div>
      <ol className="grid gap-2 sm:grid-cols-3">
        {AGENT_TASK_EMPTY_STEPS.map(({ title, description, Icon }) => (
          <li key={title} className="rounded-lg bg-black/[0.03] p-3 dark:bg-white/[0.04]">
            <div className="mb-2 flex items-center gap-2 text-foreground-light dark:text-foreground-dark">
              <Icon size={14} strokeWidth={2.2} className="shrink-0 text-apple-blue" />
              <p className="text-ui-caption font-semibold">{title}</p>
            </div>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {description}
            </p>
          </li>
        ))}
      </ol>
      <p className="mt-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
        Success looks like a task showing Waiting to start or Doing now in this list.
      </p>
    </section>
  )
}

function AgentTasksFilterEmptyState({ onReset }: { onReset: () => void }) {
  return (
    <div
      data-testid="agent-tasks-filter-empty"
      className={cn(
        'rounded-xl border border-dashed border-black/[0.1] bg-white px-4 py-5',
        'text-center text-ui-body text-secondary-light dark:border-white/[0.12] dark:bg-[#2c2c2e] dark:text-secondary-dark'
      )}
    >
      <p className="font-medium text-foreground-light dark:text-foreground-dark">
        No tasks match this view.
      </p>
      <p className="mt-1 text-ui-caption">Try All or clear the search to see this agent's tasks.</p>
      <button
        type="button"
        onClick={onReset}
        className={cn(
          'mt-3 inline-flex h-8 items-center justify-center rounded-lg px-3 text-ui-caption font-medium',
          'bg-apple-blue text-white transition-colors hover:bg-apple-blue/90',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35'
        )}
      >
        Clear filters
      </button>
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
  const failurePreview =
    task.state === 'failed' && task.error ? taskFailurePreview(task.error) : null
  const blockedPreview =
    task.state === 'blocked' && task.blockedHint
      ? taskBlockedPreview({
          blockedHint: task.blockedHint,
          blockedReason: task.blockedReason,
          error: task.error,
        })
      : null

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

      {blockedPreview && (
        <p
          data-testid={`agent-task-blocked-${task.id}`}
          className="flex items-start gap-1 text-ui-caption font-medium text-apple-red"
          title={blockedPreview}
        >
          <AlertTriangle size={12} strokeWidth={2.25} className="mt-0.5 shrink-0" />
          <span className="line-clamp-2">Needs help: {blockedPreview}</span>
        </p>
      )}

      {failurePreview && (
        <p
          data-testid={`agent-task-error-${task.id}`}
          className="line-clamp-1 text-ui-caption text-apple-red"
          title={failurePreview}
        >
          {failurePreview}
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
