import { useRef, type MouseEvent, type PointerEvent } from 'react'
import { useDraggable } from '@dnd-kit/core'
import { Brain, Send, WandSparkles } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { taskMachineKey, taskPriorityLabel, taskStateLabel } from '@app/entities/task'
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

const PRIORITY_STYLES: Record<string, string> = {
  urgent: 'border-apple-red/20 bg-apple-red/10 text-apple-red',
  high: 'border-black/[0.08] bg-white text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark',
  low: 'border-black/[0.08] bg-white text-secondary-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark',
}

interface TaskCardProps {
  task: TaskSummary
  onClick?: () => void
  onPublish?: (task: TaskSummary) => void
  displayMode?: 'comfortable' | 'compact'
}

export function TaskCard({ task, onClick, onPublish, displayMode = 'comfortable' }: TaskCardProps) {
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
  const hasAssignee = Boolean(task.assignedAgentName || task.assignedTo)
  const hasBrief = taskHasBrief(task)
  const stateKey = taskMachineKey(task.state)
  const stateLabel = taskStateLabel(task.state)
  const priorityKey = taskMachineKey(task.priority)
  const canPublish =
    task.state === 'backlog' ||
    task.state === 'queued' ||
    (task.state === 'blocked' && task.blockedReason === 'waiting_agent')
  const compact = displayMode === 'compact'
  const nextStep = compact
    ? null
    : taskNextStep(task, {
        canOpenPublishPreview: canPublish && Boolean(onPublish),
        hasAssignee,
        hasBrief,
        resultCount: resultArtifacts.length,
      })
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
  const showPriorityBadge = priorityKey !== 'normal'

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
        'cursor-pointer rounded-card border border-black/[0.08] bg-white text-left dark:border-white/[0.1] dark:bg-[#2c2c2e]',
        compact ? 'p-2.5' : 'p-3',
        'transition-colors hover:border-apple-blue/30 hover:bg-white dark:hover:border-apple-blue/35 dark:hover:bg-white/[0.05]',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
        isDragging && 'opacity-50'
      )}
    >
      <div className={cn('flex items-center justify-between', compact ? 'mb-1.5' : 'mb-2')}>
        <div className="flex items-center gap-1.5">
          <div
            className={cn('h-1.5 w-1.5 rounded-full', STATE_DOTS[stateKey] ?? STATE_DOTS.backlog)}
          />
          <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {stateLabel}
          </span>
        </div>
        <div className="flex items-center gap-1">
          {showPriorityBadge && (
            <span
              className={cn(
                'rounded-full border px-2 py-0.5 text-ui-caption font-medium',
                PRIORITY_STYLES[priorityKey] ?? PRIORITY_STYLES.low
              )}
            >
              {taskPriorityLabel(task.priority)}
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

      <p
        className={cn(
          'text-ui-body font-medium text-foreground-light dark:text-foreground-dark',
          compact ? 'mb-1.5 line-clamp-1' : 'mb-2 line-clamp-2'
        )}
      >
        {task.params.task}
      </p>

      {showProgress && !compact && (
        <div data-testid="progress-bar" className="mb-2">
          <div className="h-1 overflow-hidden rounded-full bg-apple-gray-5 dark:bg-white/10">
            <div
              className="h-full rounded-full bg-apple-blue transition-[width]"
              style={{ width: `${task.progress}%` }}
            />
          </div>
        </div>
      )}

      {failurePreview && (
        <p
          data-testid="task-error-preview"
          className="mb-1.5 line-clamp-1 text-ui-caption font-medium text-apple-red"
          title={failurePreview}
        >
          {failurePreview}
        </p>
      )}

      {blockedPreview && (
        <p
          data-testid={`task-blocked-hint-${task.id}`}
          className="mb-1.5 flex items-start gap-1 text-ui-caption font-medium text-apple-red"
          title={blockedPreview}
        >
          <span aria-hidden="true">⚠</span>
          <span>{blockedPreview}</span>
        </p>
      )}

      {nextStep && (
        <p
          data-testid="task-next-step"
          className="mb-1.5 line-clamp-2 text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {nextStep}
        </p>
      )}

      <div className="flex items-center justify-between gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
        {hasAssignee ? (
          <span className="truncate font-medium text-foreground-light dark:text-foreground-dark">
            {task.assignedAgentName ?? 'Assigned agent'}
          </span>
        ) : (
          <span>Needs agent</span>
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
          <span>Updated {formatRelativeTime(task.updatedAt || task.createdAt)}</span>
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

interface TaskNextStepOptions {
  canOpenPublishPreview: boolean
  hasAssignee: boolean
  hasBrief: boolean
  resultCount: number
}

function taskNextStep(task: TaskSummary, options: TaskNextStepOptions): string | null {
  switch (task.state) {
    case 'backlog':
      if (!options.hasBrief) {
        return options.hasAssignee
          ? 'Open this card and add details before publishing.'
          : 'Open this card, add details, then choose an agent.'
      }
      if (!options.hasAssignee) {
        return options.canOpenPublishPreview
          ? 'Choose an agent, then preview and publish.'
          : 'Choose an agent before this task can start.'
      }
      return options.canOpenPublishPreview
        ? 'Review saved items, then publish.'
        : 'Open this card, add details, then send it to an agent.'
    case 'queued':
      return options.hasAssignee
        ? 'Waiting for the chosen agent to start. If it stays here, open details or choose another agent.'
        : 'Waiting for an available agent to start. If it stays here, choose or start an agent.'
    case 'working':
      return 'Open details for live output and recent updates.'
    case 'blocked':
      if (task.blockedHint) return null
      if (task.blockedReason === 'waiting_agent') {
        return 'Attach or free an agent, then publish again.'
      }
      return 'Open details to see what is blocking this task.'
    case 'failed':
      return 'Open details, review the recovery note, then retry.'
    case 'completed':
      return options.resultCount > 0
        ? 'Open details, review result files, then save repeatable steps or create a follow-up task.'
        : 'Open details, check the final answer, then save repeatable steps or create a follow-up task.'
    case 'canceled':
      return 'Open details to see why it was canceled.'
    default:
      return 'Open details to check the current status before taking action.'
  }
}

function taskHasBrief(task: TaskSummary): boolean {
  return task.params.message?.trim().length > 0
}

function formatContextCountsLabel(counts: TaskContextCounts): string {
  const parts = []
  if (counts.appliedMemories > 0) {
    parts.push(
      `${counts.appliedMemories} saved ${counts.appliedMemories === 1 ? 'note' : 'notes'} added`
    )
  }
  if (counts.appliedSkills > 0) {
    parts.push(
      `${counts.appliedSkills} saved ${counts.appliedSkills === 1 ? 'instruction' : 'instructions'} added`
    )
  }
  return parts.join(', ')
}
