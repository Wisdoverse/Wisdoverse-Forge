import { useRef, type MouseEvent, type PointerEvent } from 'react'
import { useDraggable } from '@dnd-kit/core'
import { Brain, Send, WandSparkles } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import {
  taskResultArtifacts,
  type TaskContextCounts,
  type TaskSummary,
} from '@app/shared/api/orchestration'

const STATE_DOTS: Record<string, string> = {
  backlog: 'bg-apple-gray-2',
  queued: 'bg-apple-gray-1',
  working: 'bg-foreground-light dark:bg-foreground-dark',
  blocked: 'bg-apple-red',
  completed: 'bg-apple-gray-2',
  failed: 'bg-apple-red',
  canceled: 'bg-apple-gray-3',
}

const PRIORITY_LABELS: Record<string, string> = {
  urgent: 'Urgent',
  high: 'High',
  normal: 'Normal',
  low: 'Low',
}

const PRIORITY_STYLES: Record<string, string> = {
  urgent: 'border-apple-red/20 bg-apple-red/10 text-apple-red',
  high: 'border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  low: 'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
}

interface TaskCardProps {
  task: TaskSummary
  onClick?: () => void
  onPublish?: (task: TaskSummary) => void
}

export function TaskCard({ task, onClick, onPublish }: TaskCardProps) {
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: task.id,
  })
  const pointerStart = useRef<{ x: number; y: number } | null>(null)
  const suppressNextClick = useRef(false)

  const dragStyle = transform
    ? { transform: `translate3d(${transform.x}px, ${transform.y}px, 0)` }
    : undefined

  const showProgress = task.state === 'working' && task.progress > 0
  const resultArtifacts = taskResultArtifacts(task.result)
  const contextCounts = normalizedContextCounts(task.contextCounts)
  const showContextBadge = contextCounts.total > 0
  const canPublish =
    task.state === 'backlog' ||
    task.state === 'queued' ||
    (task.state === 'blocked' && task.blockedReason === 'waiting_agent')

  function trackPressStart(e: PointerEvent<HTMLDivElement> | MouseEvent<HTMLDivElement>) {
    if (e.button !== 0) return
    pointerStart.current = { x: e.clientX, y: e.clientY }
  }

  function activateFromPress(e: PointerEvent<HTMLDivElement> | MouseEvent<HTMLDivElement>) {
    if (!onClick || !pointerStart.current) return
    const dx = Math.abs(e.clientX - pointerStart.current.x)
    const dy = Math.abs(e.clientY - pointerStart.current.y)
    pointerStart.current = null

    if (isDragging || dx > 6 || dy > 6) return
    suppressNextClick.current = true
    onClick()
  }

  return (
    <div
      ref={setNodeRef}
      style={dragStyle}
      data-testid={`task-card-${task.id}`}
      onClick={() => {
        if (suppressNextClick.current) {
          suppressNextClick.current = false
          return
        }
        onClick?.()
      }}
      onPointerDownCapture={trackPressStart}
      onPointerUpCapture={activateFromPress}
      onPointerCancelCapture={() => {
        pointerStart.current = null
      }}
      onMouseDownCapture={trackPressStart}
      onMouseUpCapture={activateFromPress}
      {...listeners}
      {...attributes}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onClick?.()
      }}
      className={cn(
        'cursor-pointer rounded-card border border-black/[0.08] bg-white p-3 text-left dark:border-white/[0.1] dark:bg-[#2c2c2e]',
        'transition-colors hover:border-apple-blue/30 hover:bg-white dark:hover:border-apple-blue/35 dark:hover:bg-white/[0.05]',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
        isDragging && 'opacity-50'
      )}
    >
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-1.5">
          <div className={cn('h-1.5 w-1.5 rounded-full', STATE_DOTS[task.state])} />
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {task.id.slice(0, 8)}
          </span>
        </div>
        <div className="flex items-center gap-1">
          {task.priority !== 'normal' && (
            <span
              className={cn(
                'rounded-full border px-2 py-0.5 text-ui-caption font-medium',
                PRIORITY_STYLES[task.priority] ?? PRIORITY_STYLES.low
              )}
            >
              {PRIORITY_LABELS[task.priority]}
            </span>
          )}
          {canPublish && onPublish && (
            <button
              type="button"
              aria-label={`Publish ${task.params.task}`}
              title="Preview and publish"
              onPointerDown={(event) => event.stopPropagation()}
              onMouseDown={(event) => event.stopPropagation()}
              onClick={(event) => {
                event.stopPropagation()
                onPublish(task)
              }}
              className="flex h-7 w-7 items-center justify-center rounded-full text-apple-blue transition-colors hover:bg-apple-blue/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
            >
              <Send size={13} strokeWidth={2} aria-hidden="true" />
            </button>
          )}
        </div>
      </div>

      <p className="mb-2 line-clamp-2 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        {task.params.task}
      </p>

      {showProgress && (
        <div data-testid="progress-bar" className="mb-2">
          <div className="h-1 overflow-hidden rounded-full bg-apple-gray-5 dark:bg-white/10">
            <div
              className="h-full rounded-full bg-apple-blue transition-[width]"
              style={{ width: `${task.progress}%` }}
            />
          </div>
        </div>
      )}

      {task.state === 'failed' && task.error && (
        <p
          data-testid="task-error-preview"
          className="mb-1.5 line-clamp-1 text-ui-caption font-medium text-apple-red"
          title={task.error}
        >
          {task.error}
        </p>
      )}

      {task.state === 'blocked' && task.blockedHint && (
        <p
          data-testid={`task-blocked-hint-${task.id}`}
          className="mb-1.5 flex items-start gap-1 text-ui-caption font-medium text-apple-red"
          title={task.blockedHint}
        >
          <span aria-hidden="true">⚠</span>
          <span>{task.blockedHint}</span>
        </p>
      )}

      <div className="flex items-center justify-between gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        {task.assignedAgentName ? (
          <span className="truncate font-medium text-foreground-light dark:text-foreground-dark">
            {task.assignedAgentName}
          </span>
        ) : (
          <span>No assignee</span>
        )}
        <span className="flex shrink-0 items-center gap-1.5">
          {showContextBadge && (
            <span
              data-testid="task-context-badge"
              className="inline-flex h-5 shrink-0 items-center gap-1 rounded-full bg-apple-blue/10 px-1.5 text-ui-caption font-medium tabular-nums text-apple-blue"
              title={formatContextCountsLabel(contextCounts)}
              aria-label={formatContextCountsLabel(contextCounts)}
            >
              {contextCounts.appliedMemories > 0 && (
                <span className="inline-flex items-center gap-0.5">
                  <Brain size={10} strokeWidth={2} aria-hidden="true" />
                  {contextCounts.appliedMemories}
                </span>
              )}
              {contextCounts.appliedSkills > 0 && (
                <span className="inline-flex items-center gap-0.5">
                  <WandSparkles size={10} strokeWidth={2} aria-hidden="true" />
                  {contextCounts.appliedSkills}
                </span>
              )}
            </span>
          )}
          {task.state === 'completed' && resultArtifacts.length > 0 && (
            <span
              data-testid="task-result-count"
              className="text-apple-blue font-medium"
              title={`${resultArtifacts.length} attachment${resultArtifacts.length === 1 ? '' : 's'}`}
            >
              {resultArtifacts.length} file{resultArtifacts.length === 1 ? '' : 's'}
            </span>
          )}
          <span>{formatRelativeTime(task.createdAt)}</span>
        </span>
      </div>
    </div>
  )
}

function normalizedContextCounts(counts?: TaskContextCounts): TaskContextCounts {
  const appliedMemories = nonNegativeCount(counts?.appliedMemories)
  const appliedSkills = nonNegativeCount(counts?.appliedSkills)
  return {
    appliedMemories,
    appliedSkills,
    total: nonNegativeCount(counts?.total ?? appliedMemories + appliedSkills),
  }
}

function nonNegativeCount(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0
  return Math.max(0, Math.trunc(value))
}

function formatContextCountsLabel(counts: TaskContextCounts): string {
  const parts = []
  if (counts.appliedMemories > 0) {
    parts.push(
      `${counts.appliedMemories} applied ${counts.appliedMemories === 1 ? 'memory' : 'memories'}`
    )
  }
  if (counts.appliedSkills > 0) {
    parts.push(`${counts.appliedSkills} applied ${counts.appliedSkills === 1 ? 'skill' : 'skills'}`)
  }
  return parts.join(', ')
}
