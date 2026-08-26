import { useDroppable } from '@dnd-kit/core'
import { cn } from '@app/shared/lib/utils'
import { useState } from 'react'
import type { HumanMark, TaskSummary } from '@app/shared/api/orchestration'
import { TaskCard } from './TaskCard'
import { QuickCreate } from './QuickCreate'

type BoardDisplayMode = 'comfortable' | 'compact'

/** Progressive disclosure: render at most this many cards per column before
 * the "Show all" affordance, keeping very large boards responsive. */
const MAX_CARDS_PER_COLUMN = 60

const COLUMN_CONFIG: Record<string, { label: string; dot: string; surface: string }> = {
  backlog: {
    label: 'Not sent yet',
    dot: 'bg-apple-gray-2',
    surface: 'bg-white/70 dark:bg-white/[0.03]',
  },
  queued: {
    label: 'Waiting to start',
    dot: 'bg-apple-blue',
    surface: 'bg-apple-blue/[0.035] dark:bg-apple-blue/[0.06]',
  },
  working: {
    label: 'Working',
    dot: 'bg-foreground-light dark:bg-foreground-dark',
    surface: 'bg-apple-green/[0.04] dark:bg-apple-green/[0.08]',
  },
  blocked: {
    label: 'Needs help',
    dot: 'bg-apple-red',
    surface: 'bg-apple-red/[0.045] dark:bg-apple-red/[0.08]',
  },
  done: {
    label: 'Done',
    dot: 'bg-apple-green',
    surface: 'bg-apple-green/[0.035] dark:bg-apple-green/[0.06]',
  },
  failed: {
    label: 'Check retry steps',
    dot: 'bg-apple-red',
    surface: 'bg-apple-red/[0.04] dark:bg-apple-red/[0.07]',
  },
  canceled: {
    label: 'Canceled',
    dot: 'bg-apple-gray-3',
    surface: 'bg-black/[0.025] dark:bg-white/[0.035]',
  },
}

const COLUMN_EMPTY_STATE: Record<string, { title: string; detail: string }> = {
  backlog: {
    title: 'Add the first task below',
    detail: 'Add a task below with the result you want the agent to finish.',
  },
  queued: {
    title: 'Send a task to an agent first',
    detail:
      'After you choose an agent and send a task, it waits here until it starts. Open the card here if it does not start.',
  },
  working: {
    title: 'Start a waiting task to show live work',
    detail:
      'Open a task in Waiting to start, choose an agent, then choose Preview and send. Running tasks show here after work begins.',
  },
  blocked: {
    title: 'Answer help requests from this column',
    detail:
      'When a task needs details, open its card here, read what the agent needs, then choose Allow and continue.',
  },
  done: {
    title: 'Check finished work before using it',
    detail:
      'Open a completed card, check the result, then save repeatable steps or create a follow-up task.',
  },
  failed: {
    title: 'Retry stopped work from this column',
    detail:
      'Open a stopped card here, read the recovery note, then retry when the next step is clear.',
  },
  canceled: {
    title: 'Check canceled work before starting again',
    detail: 'Open a canceled card here to see why it stopped before you create a replacement task.',
  },
}

interface KanbanColumnProps {
  columnId: string
  tasks: TaskSummary[]
  onTaskClick?: (taskId: string) => void
  onTaskPublish?: (task: TaskSummary) => void
  onQuickCreate?: (
    title: string,
    columnId: string
  ) => void | boolean | string | Promise<void | boolean | string>
  displayMode?: BoardDisplayMode
  /** Latest human blocker/unblock signal per task (board badges). */
  humanMarks?: Record<string, HumanMark>
}

export function KanbanColumn({
  columnId,
  tasks,
  onTaskClick,
  onTaskPublish,
  onQuickCreate,
  displayMode = 'comfortable',
  humanMarks,
}: KanbanColumnProps) {
  const { setNodeRef, isOver } = useDroppable({ id: columnId })
  const config = COLUMN_CONFIG[columnId] ?? {
    label: columnId,
    dot: 'bg-apple-gray-2',
    surface: 'bg-white/70 dark:bg-white/[0.03]',
  }
  const [showAll, setShowAll] = useState(false)
  const visibleTasks = showAll ? tasks : tasks.slice(0, MAX_CARDS_PER_COLUMN)
  const hiddenCount = tasks.length - visibleTasks.length

  return (
    <div
      ref={setNodeRef}
      className={cn(
        'flex min-w-0 flex-none flex-col rounded-card border border-black/[0.08] p-3 dark:border-white/[0.1] md:min-w-[220px] md:flex-1',
        config.surface,
        isOver && 'ring-2 ring-apple-blue/30'
      )}
    >
      <div className="mb-3 flex items-center gap-2 px-1">
        <span className={cn('h-2 w-2 rounded-full', config.dot)} aria-hidden="true" />
        <span className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
          {config.label}
        </span>
        <span
          data-testid={`column-count-${columnId}`}
          className="ml-auto rounded-full bg-black/[0.04] px-2 py-0.5 text-ui-caption tabular-nums text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark"
        >
          {tasks.length}
        </span>
      </div>
      <div className="flex flex-col gap-2 overflow-visible md:flex-1 md:overflow-y-auto">
        {visibleTasks.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
            onClick={() => onTaskClick?.(task.id)}
            onPublish={onTaskPublish}
            displayMode={displayMode}
            humanMark={humanMarks?.[task.id]}
          />
        ))}
        {tasks.length === 0 && <ColumnEmptyState columnId={columnId} label={config.label} />}
        {hiddenCount > 0 && (
          <button
            type="button"
            data-testid={`column-show-all-${columnId}`}
            onClick={() => setShowAll(true)}
            className="rounded-button border border-dashed border-black/[0.12] px-2 py-1.5 text-ui-caption font-medium text-secondary-light transition-colors hover:border-apple-blue/40 hover:text-foreground-light dark:border-white/[0.14] dark:text-secondary-dark dark:hover:text-foreground-dark"
          >
            Show all {tasks.length} in this group
          </button>
        )}
      </div>
      {/* Quick-add only on backlog. Other columns reflect task state and
          can't accept manual inserts — promote a backlog task by dragging instead. */}
      {columnId === 'backlog' && (
        <QuickCreate columnId={columnId} onSubmit={(title, col) => onQuickCreate?.(title, col)} />
      )}
    </div>
  )
}

function ColumnEmptyState({ columnId, label }: { columnId: string; label: string }) {
  const emptyState = COLUMN_EMPTY_STATE[columnId] ?? {
    title: `Open ${label.toLowerCase()} tasks to check next steps`,
    detail: 'When a task reaches this board step, open its card to see what to do next.',
  }

  return (
    <div
      data-testid={`kanban-empty-${columnId}`}
      className="rounded-card border border-dashed border-black/10 px-3 py-4 text-center dark:border-white/10"
    >
      <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
        {emptyState.title}
      </p>
      <p className="mt-1 text-ui-caption leading-snug text-secondary-light dark:text-secondary-dark">
        {emptyState.detail}
      </p>
    </div>
  )
}
