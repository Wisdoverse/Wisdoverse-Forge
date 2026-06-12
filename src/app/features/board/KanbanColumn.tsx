import { useDroppable } from '@dnd-kit/core'
import { cn } from '@app/shared/lib/utils'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { TaskCard } from './TaskCard'
import { QuickCreate } from './QuickCreate'

type BoardDisplayMode = 'comfortable' | 'compact'

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
    label: 'Blocked',
    dot: 'bg-apple-red',
    surface: 'bg-apple-red/[0.045] dark:bg-apple-red/[0.08]',
  },
  done: {
    label: 'Done',
    dot: 'bg-apple-green',
    surface: 'bg-apple-green/[0.035] dark:bg-apple-green/[0.06]',
  },
  failed: {
    label: 'Needs review',
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
    title: 'No tasks waiting to send',
    detail: 'Add a draft task below with the result you want the agent to finish.',
  },
  queued: {
    title: 'Nothing waiting to start',
    detail: 'Assigned tasks wait here until the next ready agent starts them.',
  },
  working: {
    title: 'No active runs',
    detail: 'Running work appears here once an agent starts the task.',
  },
  blocked: {
    title: 'No blockers',
    detail: 'Tasks needing owner input or missing details will collect here.',
  },
  done: {
    title: 'Nothing ready for review',
    detail: 'Completed tasks move here so you can check results and saved guidance.',
  },
  failed: {
    title: 'No work needing recovery',
    detail: 'If a task stops early, open its card here to review the recovery note and retry path.',
  },
  canceled: {
    title: 'No canceled tasks',
    detail: 'Canceled work stays here so the board keeps its history visible.',
  },
}

interface KanbanColumnProps {
  columnId: string
  tasks: TaskSummary[]
  onTaskClick?: (taskId: string) => void
  onTaskPublish?: (task: TaskSummary) => void
  onQuickCreate?: (title: string, columnId: string) => void
  displayMode?: BoardDisplayMode
}

export function KanbanColumn({
  columnId,
  tasks,
  onTaskClick,
  onTaskPublish,
  onQuickCreate,
  displayMode = 'comfortable',
}: KanbanColumnProps) {
  const { setNodeRef, isOver } = useDroppable({ id: columnId })
  const config = COLUMN_CONFIG[columnId] ?? {
    label: columnId,
    dot: 'bg-apple-gray-2',
    surface: 'bg-white/70 dark:bg-white/[0.03]',
  }

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
        {tasks.map((task) => (
          <TaskCard
            key={task.id}
            task={task}
            onClick={() => onTaskClick?.(task.id)}
            onPublish={onTaskPublish}
            displayMode={displayMode}
          />
        ))}
        {tasks.length === 0 && <ColumnEmptyState columnId={columnId} label={config.label} />}
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
    title: `No ${label.toLowerCase()} tasks`,
    detail: 'Tasks will appear here when they reach this board step.',
  }

  return (
    <div
      data-testid={`kanban-empty-${columnId}`}
      className="rounded-lg border border-dashed border-black/10 px-3 py-4 text-center dark:border-white/10"
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
