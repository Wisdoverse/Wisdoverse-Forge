import { useRef, useMemo, useState, type ReactNode } from 'react'
import { useVirtualizer, type VirtualizerOptions } from '@tanstack/react-virtual'
import { AlertTriangle, CheckCircle2, Clock3, CircleDot, ListChecks, Search } from 'lucide-react'
import { useBoardStore } from '@app/shared/model/board.store'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime as formatDate } from '@app/shared/lib/time'
import { taskBlockedPreview } from '@app/shared/lib/taskFailureCopy'

const PRIORITY_LABELS: Record<string, string> = {
  low: 'Low',
  normal: 'Normal',
  high: 'High',
  urgent: 'Urgent',
}

const PRIORITY_COLORS: Record<string, string> = {
  low: 'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
  normal:
    'border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  high: 'border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  urgent: 'border-apple-red/20 bg-apple-red/10 text-apple-red',
}

const STATUS_LABELS: Record<string, string> = {
  backlog: 'Backlog',
  queued: 'Queued',
  working: 'Working',
  blocked: 'Blocked',
  completed: 'Done',
  failed: 'Failed',
  canceled: 'Canceled',
}

const STATUS_COLORS: Record<string, string> = {
  backlog:
    'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
  queued:
    'border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  working:
    'border-foreground-light/20 bg-white text-foreground-light dark:border-white/[0.18] dark:bg-white/[0.04] dark:text-foreground-dark',
  blocked: 'border-apple-red/20 bg-apple-red/10 text-apple-red',
  completed:
    'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
  failed: 'border-apple-red/20 bg-apple-red/10 text-apple-red',
  canceled:
    'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
}

const ROW_HEIGHT = 68

type ListTaskFilter = 'all' | 'open' | 'attention' | 'completed'

const LIST_FILTERS: { value: ListTaskFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'open', label: 'Open' },
  { value: 'attention', label: 'Needs action' },
  { value: 'completed', label: 'Completed' },
]

// Custom observeElementRect that falls back to a large virtual height in jsdom
// (where getBoundingClientRect always returns zeros)
const observeElementRectFallback: VirtualizerOptions<
  HTMLDivElement,
  HTMLDivElement
>['observeElementRect'] = (instance, cb) => {
  const el = instance.scrollElement
  if (!el) return () => undefined

  const measure = () => {
    const rect = el.getBoundingClientRect()
    cb({
      width: rect.width || 800,
      height: rect.height || 600,
    })
  }

  measure()

  if (typeof ResizeObserver !== 'undefined') {
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  }

  return () => undefined
}

export function ListView() {
  const { columns, setSelectedTask } = useBoardStore()
  const [searchQuery, setSearchQuery] = useState('')
  const [filter, setFilter] = useState<ListTaskFilter>('all')

  const tasks = useMemo<TaskSummary[]>(() => Object.values(columns).flat(), [columns])
  const workload = useMemo(() => summarizeListTasks(tasks), [tasks])
  const nextStep = listNextStep(workload, tasks.length)
  const filterCounts = useMemo(
    () =>
      LIST_FILTERS.map((item) => ({
        ...item,
        count: tasks.filter((task) => taskMatchesListFilter(task, item.value)).length,
      })),
    [tasks]
  )
  const visibleTasks = useMemo(
    () => filterListTasks(tasks, filter, searchQuery),
    [filter, searchQuery, tasks]
  )
  const hasActiveFilter = searchQuery.trim().length > 0 || filter !== 'all'

  const scrollRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: visibleTasks.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
    observeElementRect: observeElementRectFallback,
  })

  return (
    <div className="flex h-full flex-col gap-3">
      <section
        data-testid="list-work-register"
        className="rounded-lg border border-black/[0.08] bg-white px-3 py-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
      >
        <div className="mb-3 flex items-start gap-3">
          <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
            <ListChecks size={17} strokeWidth={2.1} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h2 className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
              Task List
            </h2>
            <p
              data-testid="list-next-step"
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              <span className="font-medium text-foreground-light dark:text-foreground-dark">
                {nextStep.title}
              </span>{' '}
              {nextStep.detail}
            </p>
          </div>
        </div>
        <div className="flex flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4 xl:w-[560px]">
            <ListMetric
              testId="list-metric-active"
              label="Active"
              value={workload.active}
              icon={<CircleDot size={15} strokeWidth={2.15} aria-hidden="true" />}
              tone="active"
            />
            <ListMetric
              testId="list-metric-backlog"
              label="Backlog"
              value={workload.backlog}
              icon={<Clock3 size={15} strokeWidth={2.15} aria-hidden="true" />}
              tone="neutral"
            />
            <ListMetric
              testId="list-metric-attention"
              label="Needs action"
              value={workload.attention}
              icon={<AlertTriangle size={15} strokeWidth={2.15} aria-hidden="true" />}
              tone="attention"
            />
            <ListMetric
              testId="list-metric-completed"
              label="Completed"
              value={workload.completed}
              icon={<CheckCircle2 size={15} strokeWidth={2.15} aria-hidden="true" />}
              tone="success"
            />
          </div>

          <div className="flex min-w-0 flex-1 flex-col gap-2 lg:flex-row lg:items-center lg:justify-end">
            <label className="relative min-w-0 flex-1 lg:max-w-sm">
              <span className="sr-only">Search task list</span>
              <Search
                size={15}
                strokeWidth={2}
                className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
                aria-hidden="true"
              />
              <input
                data-testid="list-search"
                type="search"
                value={searchQuery}
                onChange={(event) => setSearchQuery(event.target.value)}
                placeholder="Search tasks, agents, blockers…"
                className="h-9 w-full rounded-lg border border-black/[0.08] bg-black/[0.02] pl-9 pr-3 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light focus:border-apple-blue/40 focus:bg-white focus:ring-2 focus:ring-apple-blue/20 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark dark:focus:bg-white/[0.06]"
              />
            </label>

            <div
              role="group"
              aria-label="List task filter"
              data-testid="list-task-filter"
              className="inline-flex max-w-full items-center gap-1 overflow-x-auto rounded-lg bg-black/[0.035] p-1 dark:bg-white/[0.05]"
            >
              {filterCounts.map((item) => (
                <ListFilterButton
                  key={item.value}
                  active={filter === item.value}
                  label={item.label}
                  count={item.count}
                  onClick={() => setFilter(item.value)}
                />
              ))}
            </div>

            <span className="shrink-0 text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
              {visibleTasks.length}/{tasks.length} tasks
            </span>
          </div>
        </div>
      </section>

      {/* Table header */}
      <div className="flex-shrink-0 overflow-x-auto border-b border-black/[0.06] dark:border-white/[0.06]">
        <div className="grid min-w-[720px] select-none grid-cols-[minmax(220px,1fr)_120px_140px_96px_96px] px-4 py-2 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          <span>Title</span>
          <span>Status</span>
          <span>Assignee</span>
          <span>Priority</span>
          <span>Updated</span>
        </div>
      </div>

      {tasks.length === 0 ? (
        <div
          data-testid="list-empty-state"
          className="flex flex-1 flex-col items-center justify-center gap-2 px-4 text-center text-ui-body"
        >
          <div className="flex size-10 items-center justify-center rounded-lg bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
            <ListChecks size={18} strokeWidth={1.9} aria-hidden="true" />
          </div>
          <p className="font-semibold text-foreground-light dark:text-foreground-dark">
            No tasks yet
          </p>
          <p className="max-w-sm text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
            Create one small task from the board first. Start with the outcome you want, then add
            the proof you expect the agent to return.
          </p>
        </div>
      ) : visibleTasks.length === 0 ? (
        <div
          data-testid="list-filter-empty"
          className="flex flex-1 flex-col items-center justify-center gap-2 text-center text-ui-body text-secondary-light dark:text-secondary-dark"
        >
          <span className="font-medium text-foreground-light dark:text-foreground-dark">
            No tasks match this view
          </span>
          <span className="max-w-sm text-ui-caption leading-relaxed">
            Show all tasks first, then narrow by task title, agent name, blocker, or priority.
          </span>
          {hasActiveFilter && (
            <button
              type="button"
              onClick={() => {
                setSearchQuery('')
                setFilter('all')
              }}
              className="rounded-full bg-apple-blue/10 px-3 py-1.5 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
            >
              Show all tasks
            </button>
          )}
        </div>
      ) : (
        /* Scrollable virtualised body */
        <div ref={scrollRef} className="flex-1 overflow-auto" style={{ contain: 'strict' }}>
          <div
            className="min-w-[720px]"
            style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const task = visibleTasks[virtualRow.index]
              const openTask = () => setSelectedTask(task.id)
              const nextAction = taskNextAction(task)
              return (
                <div
                  key={task.id}
                  data-index={virtualRow.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: 'absolute',
                    top: virtualRow.start,
                    left: 0,
                    right: 0,
                    height: ROW_HEIGHT,
                  }}
                  className="grid cursor-pointer grid-cols-[minmax(220px,1fr)_120px_140px_96px_96px] items-center border-b border-black/[0.05] px-4 transition-colors hover:bg-black/[0.025] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.06] dark:hover:bg-white/[0.04]"
                  role="button"
                  tabIndex={0}
                  aria-label={`Open ${task.params.task}. ${nextAction}`}
                  onClick={openTask}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault()
                      openTask()
                    }
                  }}
                >
                  {/* Title */}
                  <span className="min-w-0 pr-4">
                    <span className="block truncate text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                      {task.params.task}
                    </span>
                    <span className="mt-0.5 block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                      {nextAction}
                    </span>
                  </span>

                  {/* Status badge */}
                  <span
                    className={cn(
                      'inline-flex w-fit items-center rounded-full border px-2 py-0.5 text-ui-caption font-medium',
                      STATUS_COLORS[task.state] ?? STATUS_COLORS.backlog
                    )}
                  >
                    {STATUS_LABELS[task.state] ?? task.state}
                  </span>

                  {/* Assignee */}
                  <span className="truncate text-ui-body text-secondary-light dark:text-secondary-dark">
                    {task.assignedAgentName ?? task.assignedTo ?? '—'}
                  </span>

                  {/* Priority badge */}
                  <span
                    className={cn(
                      'inline-flex w-fit items-center rounded-full border px-2 py-0.5 text-ui-caption font-medium',
                      PRIORITY_COLORS[task.priority] ?? PRIORITY_COLORS.normal
                    )}
                  >
                    {PRIORITY_LABELS[task.priority] ?? task.priority}
                  </span>

                  {/* Updated */}
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {formatDate(task.updatedAt)}
                  </span>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}

function ListMetric({
  testId,
  label,
  value,
  icon,
  tone,
}: {
  testId: string
  label: string
  value: number
  icon: ReactNode
  tone: 'active' | 'attention' | 'neutral' | 'success'
}) {
  const toneClass =
    tone === 'active'
      ? 'text-apple-blue'
      : tone === 'attention'
        ? 'text-apple-orange'
        : tone === 'success'
          ? 'text-apple-green'
          : 'text-secondary-light dark:text-secondary-dark'

  return (
    <div
      data-testid={testId}
      className="flex min-h-14 items-center gap-2 rounded-lg bg-black/[0.025] px-2.5 py-2 dark:bg-white/[0.05]"
    >
      <span
        className={cn(
          'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white dark:bg-black/20',
          toneClass
        )}
      >
        {icon}
      </span>
      <span className="min-w-0">
        <span className="block text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
          {value}
        </span>
        <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {label}
        </span>
      </span>
    </div>
  )
}

function ListFilterButton({
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
        'inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-ui-caption font-medium transition-colors',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
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

function summarizeListTasks(tasks: TaskSummary[]): {
  active: number
  backlog: number
  attention: number
  completed: number
} {
  return tasks.reduce(
    (summary, task) => {
      if (task.state === 'queued' || task.state === 'working') summary.active += 1
      if (task.state === 'backlog') summary.backlog += 1
      if (task.state === 'blocked' || task.state === 'failed') summary.attention += 1
      if (task.state === 'completed') summary.completed += 1
      return summary
    },
    { active: 0, backlog: 0, attention: 0, completed: 0 }
  )
}

function listNextStep(
  workload: ReturnType<typeof summarizeListTasks>,
  totalTasks: number
): { title: string; detail: string } {
  if (totalTasks === 0) {
    return {
      title: 'Create your first small task.',
      detail: 'Use the board to give an agent one clear outcome and expected proof.',
    }
  }

  if (workload.attention > 0) {
    return {
      title: `Start with ${workload.attention} task${workload.attention === 1 ? '' : 's'} needing action.`,
      detail: 'Open the blocked or failed work first so the agent is not waiting on you.',
    }
  }

  if (workload.active > 0) {
    return {
      title: `Review ${workload.active} active task${workload.active === 1 ? '' : 's'}.`,
      detail: 'Open active work to confirm progress is moving and no decision is needed.',
    }
  }

  if (workload.backlog > 0) {
    return {
      title: `Move ${workload.backlog} backlog task${workload.backlog === 1 ? '' : 's'} forward.`,
      detail: 'Assign an agent or send ready work into the next lane when the scope is clear.',
    }
  }

  return {
    title: 'Review completed work.',
    detail: 'Open completed tasks to check the result, evidence, and anything worth reusing.',
  }
}

function taskNextAction(task: TaskSummary): string {
  switch (task.state) {
    case 'backlog':
      return task.assignedAgentName || task.assignedTo
        ? 'Queue this when you are ready for the agent to start.'
        : 'Assign an agent or move it into a ready work lane.'
    case 'queued':
      return 'Wait for an agent to pick it up; check again if it stays queued.'
    case 'working':
      return `Follow progress at ${task.progress}%; open it if updates stop.`
    case 'blocked':
      return `Resolve blocker: ${taskBlockedPreview({
        blockedHint: task.blockedHint,
        blockedReason: task.blockedReason,
        error: task.error,
      })}`
    case 'failed':
      return 'Open it, read the failure, then retry only after the cause is clear.'
    case 'completed':
      return 'Open it to review the result and evidence.'
    case 'canceled':
      return 'Open it only if you need to restart or explain why it stopped.'
    default:
      return 'Open the task to decide the next safe step.'
  }
}

function filterListTasks(
  tasks: TaskSummary[],
  filter: ListTaskFilter,
  query: string
): TaskSummary[] {
  const normalizedQuery = query.trim().toLowerCase()
  return tasks.filter((task) => {
    if (!taskMatchesListFilter(task, filter)) return false
    if (normalizedQuery.length === 0) return true
    return taskSearchText(task).includes(normalizedQuery)
  })
}

function taskMatchesListFilter(task: TaskSummary, filter: ListTaskFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'open')
    return task.state === 'backlog' || task.state === 'queued' || task.state === 'working'
  if (filter === 'attention') return task.state === 'blocked' || task.state === 'failed'
  return task.state === 'completed'
}

function taskSearchText(task: TaskSummary): string {
  return [
    task.params.task,
    task.params.message,
    task.assignedAgentName,
    task.assignedTo,
    task.priority,
    task.state,
    task.error,
    task.blockedHint,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}
