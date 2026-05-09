import { useDroppable } from '@dnd-kit/core'
import { cn } from '@app/shared/lib/utils'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { TaskCard } from './TaskCard'
import { QuickCreate } from './QuickCreate'

const COLUMN_CONFIG: Record<string, { label: string; dot: string }> = {
  backlog: { label: 'Backlog', dot: 'bg-apple-gray-2' },
  queued: { label: 'Queued', dot: 'bg-apple-gray-1' },
  working: { label: 'Working', dot: 'bg-foreground-light dark:bg-foreground-dark' },
  blocked: { label: 'Blocked', dot: 'bg-apple-red' },
  done: { label: 'Done', dot: 'bg-apple-gray-2' },
  failed: { label: 'Failed', dot: 'bg-apple-red' },
  canceled: { label: 'Canceled', dot: 'bg-apple-gray-3' },
}

interface KanbanColumnProps {
  columnId: string
  tasks: TaskSummary[]
  onTaskClick?: (taskId: string) => void
  onTaskPublish?: (task: TaskSummary) => void
  onQuickCreate?: (title: string, columnId: string) => void
}

export function KanbanColumn({
  columnId,
  tasks,
  onTaskClick,
  onTaskPublish,
  onQuickCreate,
}: KanbanColumnProps) {
  const { setNodeRef, isOver } = useDroppable({ id: columnId })
  const config = COLUMN_CONFIG[columnId] ?? { label: columnId, dot: 'bg-apple-gray-2' }

  return (
    <div
      ref={setNodeRef}
      className={cn(
        'flex min-w-0 flex-none flex-col rounded-card border border-black/[0.08] bg-white/70 p-3 dark:border-white/[0.1] dark:bg-white/[0.03] md:min-w-[220px] md:flex-1',
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
          />
        ))}
      </div>
      {/* Quick-add only on backlog. Other columns reflect dispatcher state and
          can't accept manual inserts — promote a backlog task by dragging instead. */}
      {columnId === 'backlog' && (
        <QuickCreate columnId={columnId} onSubmit={(title, col) => onQuickCreate?.(title, col)} />
      )}
    </div>
  )
}
