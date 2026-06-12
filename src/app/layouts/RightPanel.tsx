import { X } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { ActivityFeed } from '@app/features/feed/ActivityFeed'
import { TaskDetailPanel } from '@app/features/detail/TaskDetailPanel'
import { useBoardStore } from '@app/shared/model/board.store'

interface RightPanelProps {
  collapsed: boolean
  onToggle: () => void
  children?: React.ReactNode
  variant?: 'side' | 'mobile'
}

export function RightPanel({ collapsed, onToggle, children, variant = 'side' }: RightPanelProps) {
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
          ? 'w-full h-full flex flex-col'
          : 'w-[280px] flex-shrink-0 flex flex-col',
        'bg-surface-elevated backdrop-blur-[30px] rounded-panel',
        'shadow-panel dark:bg-surface-dark-elevated dark:shadow-panel-dark'
      )}
    >
      {selectedTask ? (
        <div className="flex-1 overflow-y-auto p-4">
          <TaskDetailPanel task={selectedTask} onClose={() => setSelectedTask(null)} />
        </div>
      ) : (
        <>
          <div className="flex items-center justify-between px-4 py-3 border-b border-black/[0.04] dark:border-white/[0.04]">
            <div className="min-w-0">
              <h2 className="text-[13px] font-semibold text-foreground-light dark:text-foreground-dark">
                Live task updates
              </h2>
              <p className="mt-0.5 truncate text-[10px] text-secondary-light dark:text-secondary-dark">
                Agent progress, help needed, and finished work
              </p>
            </div>
            <button
              onClick={onToggle}
              aria-label="Hide live task updates panel"
              className="w-6 h-6 flex items-center justify-center rounded-md text-secondary-light dark:text-secondary-dark hover:bg-black/[0.06] dark:hover:bg-white/[0.08] hover:text-foreground-light dark:hover:text-foreground-dark transition-colors"
            >
              <X size={14} strokeWidth={2} />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-4">{children ?? <ActivityFeed />}</div>
        </>
      )}
    </aside>
  )
}
