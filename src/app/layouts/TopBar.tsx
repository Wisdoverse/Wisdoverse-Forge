import { Menu, Plus, Search } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { cn } from '@app/shared/lib/utils'
import type { ViewMode } from '@app/shared/model/board.types'

interface TopBarProps {
  title: string
  subtitle?: string
  showTaskControls?: boolean
  onMenuClick?: () => void
  viewMode: ViewMode
  onViewChange: (view: ViewMode) => void
  onCreateTask: () => void
  createTaskLabel?: string
  createTaskTitle?: string
  onCmdK?: () => void
}

const VIEW_OPTIONS: { id: ViewMode; labelKey: string }[] = [
  { id: 'board', labelKey: 'appLayout.topBar.views.board' },
  { id: 'list', labelKey: 'appLayout.topBar.views.list' },
  { id: 'timeline', labelKey: 'appLayout.topBar.views.timeline' },
  { id: '3d', labelKey: 'appLayout.topBar.views.map' },
]

export function TopBar({
  title,
  subtitle,
  showTaskControls = false,
  onMenuClick,
  viewMode,
  onViewChange,
  onCreateTask,
  createTaskLabel,
  createTaskTitle,
  onCmdK,
}: TopBarProps) {
  const { t } = useTranslation()
  const taskLabel = createTaskLabel ?? t('commandPalette.taskSetup.ready.buttonLabel')
  const taskTitle = createTaskTitle ?? t('commandPalette.taskSetup.ready.description')
  return (
    <div
      data-testid="top-bar"
      className={cn(
        'flex min-h-[52px] items-center justify-between gap-4 border-b border-black/[0.08] bg-background-light px-4 py-1',
        'dark:border-white/[0.1] dark:bg-background-dark'
      )}
    >
      <div className="flex items-center gap-3 min-w-0">
        {onMenuClick && (
          <button
            type="button"
            onClick={onMenuClick}
            aria-label={t('appLayout.topBar.openNavigation')}
            className="flex h-11 w-11 items-center justify-center rounded-button text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark md:hidden"
          >
            <Menu size={18} strokeWidth={2} aria-hidden="true" />
          </button>
        )}
        <div className="min-w-0">
          <h1 className="truncate text-ui-title font-medium text-foreground-light dark:text-foreground-dark">
            {title}
          </h1>
          {subtitle && (
            <p className="hidden truncate text-ui-caption text-secondary-light dark:text-secondary-dark sm:block">
              {subtitle}
            </p>
          )}
        </div>
        {showTaskControls && (
          <div className="ml-2 hidden gap-0.5 rounded-button border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.04] md:flex">
            {VIEW_OPTIONS.map((opt) => (
              <button
                key={opt.id}
                type="button"
                onClick={() => onViewChange(opt.id)}
                className={cn(
                  'rounded-button px-2.5 py-1 text-ui-caption transition-colors',
                  viewMode === opt.id
                    ? 'bg-black/[0.06] text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark'
                    : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
                )}
              >
                {t(opt.labelKey)}
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="flex items-center gap-2 flex-shrink-0">
        {onCmdK && (
          <button
            type="button"
            data-testid="top-bar-command-search"
            onClick={onCmdK}
            aria-label={t('appLayout.topBar.searchLabel')}
            className="flex h-8 w-8 items-center justify-center rounded-button text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light active:scale-95 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
            title={t('appLayout.topBar.searchLabel')}
          >
            <Search size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        )}

        {showTaskControls && (
          <button
            type="button"
            onClick={onCreateTask}
            aria-label={taskLabel}
            title={taskTitle}
            className="inline-flex h-8 items-center gap-1.5 rounded-button bg-apple-blue px-3 text-ui-button font-medium text-white transition-colors hover:bg-apple-blue-focus active:scale-95"
          >
            <Plus size={14} strokeWidth={2.25} aria-hidden="true" />
            <span>{taskLabel}</span>
          </button>
        )}
      </div>
    </div>
  )
}
