import type { ReactNode } from 'react'
import { Menu, Moon, Sun } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useTheme } from '@app/shared/model/theme.context'
import type { ViewMode, GroupBy } from '@app/shared/model/board.types'

interface TopBarProps {
  title: string
  subtitle?: string
  showTaskControls?: boolean
  onMenuClick?: () => void
  viewMode: ViewMode
  groupBy: GroupBy
  onViewChange: (view: ViewMode) => void
  onGroupByChange: (group: GroupBy) => void
  onCreateTask: () => void
  agentGroupSelector?: ReactNode
  onCmdK?: () => void
}

const VIEW_OPTIONS: { id: ViewMode; label: string }[] = [
  { id: 'board', label: 'Board' },
  { id: 'list', label: 'List' },
  { id: 'timeline', label: 'Timeline' },
  { id: '3d', label: '3D' },
]

const GROUP_OPTIONS: { id: GroupBy; label: string }[] = [
  { id: 'status', label: 'Status' },
  { id: 'agent', label: 'Agent' },
  { id: 'priority', label: 'Priority' },
]

export function TopBar({
  title,
  subtitle,
  showTaskControls = false,
  onMenuClick,
  viewMode,
  groupBy,
  onViewChange,
  onGroupByChange,
  onCreateTask,
  agentGroupSelector,
  onCmdK,
}: TopBarProps) {
  const { theme, toggleTheme } = useTheme()
  return (
    <div
      data-testid="top-bar"
      className={cn(
        'flex min-h-[52px] items-center justify-between gap-4 px-4 py-2.5',
        'rounded-panel border border-black/[0.08] bg-surface backdrop-blur-[20px] backdrop-saturate-[180%]',
        'dark:border-white/[0.1] dark:bg-surface-dark'
      )}
    >
      <div className="flex items-center gap-3 min-w-0">
        {onMenuClick && (
          <button
            type="button"
            onClick={onMenuClick}
            aria-label="Open navigation"
            className="md:hidden w-8 h-8 flex items-center justify-center rounded-lg text-secondary-light dark:text-secondary-dark hover:bg-black/[0.04] dark:hover:bg-white/[0.06] hover:text-foreground-light dark:hover:text-foreground-dark transition-colors"
          >
            <Menu size={18} strokeWidth={2} aria-hidden="true" />
          </button>
        )}
        <div className="min-w-0">
          <h1 className="truncate text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
            {title}
          </h1>
          {subtitle && (
            <p className="hidden truncate text-ui-caption text-secondary-light dark:text-secondary-dark sm:block">
              {subtitle}
            </p>
          )}
        </div>
        {showTaskControls && (
          <div className="ml-2 hidden gap-0.5 rounded-full border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.06] md:flex">
            {VIEW_OPTIONS.map((opt) => (
              <button
                key={opt.id}
                type="button"
                onClick={() => onViewChange(opt.id)}
                className={cn(
                  'rounded-full px-3 py-1 text-ui-caption transition-transform active:scale-95',
                  viewMode === opt.id
                    ? 'bg-apple-blue text-white'
                    : 'text-secondary-light dark:text-secondary-dark hover:text-foreground-light dark:hover:text-foreground-dark'
                )}
              >
                {opt.label}
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center gap-2 flex-shrink-0">
        {showTaskControls && agentGroupSelector}

        {showTaskControls && (
          <div className="hidden gap-0.5 rounded-full border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.06] lg:flex">
            {GROUP_OPTIONS.map((opt) => (
              <button
                key={opt.id}
                type="button"
                onClick={() => onGroupByChange(opt.id)}
                className={cn(
                  'rounded-full px-3 py-1 text-ui-caption transition-transform active:scale-95',
                  groupBy === opt.id
                    ? 'bg-apple-blue text-white'
                    : 'text-secondary-light dark:text-secondary-dark'
                )}
              >
                {opt.label}
              </button>
            ))}
          </div>
        )}

        <button
          type="button"
          onClick={toggleTheme}
          aria-label={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
          className="flex h-11 w-11 items-center justify-center rounded-full text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light active:scale-95 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
          title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {theme === 'dark' ? (
            <Sun size={15} strokeWidth={2} aria-hidden="true" />
          ) : (
            <Moon size={15} strokeWidth={2} aria-hidden="true" />
          )}
        </button>
        <button
          type="button"
          onClick={onCmdK}
          className="hidden rounded-full bg-white px-3 py-1.5 text-ui-caption text-secondary-light transition-colors hover:text-foreground-light active:scale-95 dark:bg-white/[0.06] dark:text-secondary-dark dark:hover:text-foreground-dark sm:block"
          title="Command palette"
        >
          ⌘K
        </button>

        {showTaskControls && (
          <button
            type="button"
            onClick={onCreateTask}
            className="rounded-full bg-apple-blue px-4 py-2 text-ui-button font-medium text-white transition-transform hover:bg-apple-blue-focus active:scale-95"
          >
            + Task
          </button>
        )}
      </div>
    </div>
  )
}
