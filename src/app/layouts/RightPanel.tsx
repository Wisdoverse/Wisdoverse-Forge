import { X } from 'lucide-react'
import { ActivityFeed } from '@app/features/feed'

interface RightPanelProps {
  collapsed: boolean
  onToggle: () => void
  onOpenBoard?: () => void
}

export function RightPanel({ collapsed, onToggle, onOpenBoard }: RightPanelProps) {
  if (collapsed) return null

  return (
    <aside
      data-testid="right-panel"
      className="flex min-h-0 w-[280px] flex-shrink-0 flex-col overflow-hidden border-l border-black/[0.08] bg-background-light dark:border-white/[0.1] dark:bg-background-dark"
    >
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
        <ActivityFeed onOpenBoard={onOpenBoard} />
      </div>
    </aside>
  )
}
