import { useRef, useMemo } from 'react'
import { useVirtualizer, type VirtualizerOptions } from '@tanstack/react-virtual'
import { useBoardStore } from '@app/shared/model/board.store'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime as formatDate } from '@app/shared/lib/time'

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

const ROW_HEIGHT = 52

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

  const tasks = useMemo<TaskSummary[]>(() => Object.values(columns).flat(), [columns])

  const scrollRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: tasks.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
    observeElementRect: observeElementRectFallback,
  })

  return (
    <div className="flex h-full flex-col">
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
        <div className="flex flex-1 items-center justify-center text-ui-body text-secondary-light dark:text-secondary-dark">
          No Tasks Yet
        </div>
      ) : (
        /* Scrollable virtualised body */
        <div ref={scrollRef} className="flex-1 overflow-auto" style={{ contain: 'strict' }}>
          <div
            className="min-w-[720px]"
            style={{ height: virtualizer.getTotalSize(), position: 'relative' }}
          >
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const task = tasks[virtualRow.index]
              const openTask = () => setSelectedTask(task.id)
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
                  aria-label={`Open ${task.params.task}`}
                  onClick={openTask}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault()
                      openTask()
                    }
                  }}
                >
                  {/* Title */}
                  <span className="truncate pr-4 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                    {task.params.task}
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
