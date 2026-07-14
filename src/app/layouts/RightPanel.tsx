import { X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { ActivityFeed } from '@app/features/feed'
import { TaskDetailPanel } from '@app/features/detail'
import { useBoardStore } from '@app/entities/navigation/model/board.store'

interface RightPanelProps {
  collapsed: boolean
  onToggle: () => void
  onOpenBoard?: () => void
  children?: React.ReactNode
  variant?: 'side' | 'mobile'
}

export function RightPanel({
  collapsed,
  onToggle,
  onOpenBoard,
  children,
  variant = 'side',
}: RightPanelProps) {
  const { selectedTaskId, columns, setSelectedTask } = useBoardStore()

  const selectedTask = selectedTaskId
    ? (Object.values(columns)
        .flat()
        .find((t) => t.id === selectedTaskId) ?? null)
    : null

  if (collapsed) return null

  return (
    <aside
      data-testid="right-panel"
      className={cn(
        variant === 'mobile'
          ? 'flex h-full w-full flex-col rounded-card border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-surface-dark'
          : 'flex w-[280px] flex-shrink-0 flex-col border-l border-black/[0.08] bg-background-light dark:border-white/[0.1] dark:bg-background-dark',
        'min-h-0 overflow-hidden'
      )}
    >
      {selectedTask ? (
        <div className="min-h-0 flex-1 overflow-y-auto p-4 text-ui-body [&>div>h2]:text-ui-doc-title">
          <TaskDetailPanel task={selectedTask} onClose={() => setSelectedTask(null)} />
        </div>
      ) : (
        <>
          <div className="flex items-center justify-between border-b border-black/[0.08] px-4 py-3 dark:border-white/[0.1]">
            <div className="min-w-0">
              <h2 className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                Live task updates
              </h2>
              <p className="mt-0.5 truncate text-ui-body text-secondary-light dark:text-secondary-dark">
                Agent progress, help needed, and finished task results
              </p>
            </div>
            <button
              onClick={onToggle}
              aria-label="Hide live task updates"
              className="flex h-8 w-8 items-center justify-center rounded-button text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
            >
              <X size={16} strokeWidth={2} />
            </button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-4 text-ui-body">
            {children ?? <ActivityFeed onOpenBoard={onOpenBoard} />}
          </div>
        </>
      )}
    </aside>
  )
}
