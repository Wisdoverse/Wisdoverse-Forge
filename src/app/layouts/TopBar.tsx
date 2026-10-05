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
        'flex min-h-[52px] flex-col gap-2 border-b border-black/[0.08] bg-background-light px-3 py-2 sm:px-4 @[48rem]/workspace:flex-row @[48rem]/workspace:items-center @[48rem]/workspace:justify-between',
        'dark:border-white/[0.1] dark:bg-background-dark'
      )}
    >
      <div className="flex min-w-0 flex-1 items-center gap-3">
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
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-ui-title font-semibold tracking-[-0.01em] text-foreground-light dark:text-foreground-dark">
            {title}
          </h1>
          {subtitle && (
            <p className="hidden truncate text-ui-caption text-secondary-light dark:text-secondary-dark sm:block">
              {subtitle}
            </p>
          )}
        </div>
        {onCmdK && (
          <button
            type="button"
            data-testid="top-bar-command-search"
            onClick={onCmdK}
            aria-label={t('appLayout.topBar.searchLabel')}
            className={cn(
              'flex h-11 w-11 items-center justify-center rounded-button text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light active:scale-95 dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark',
              'shrink-0'
            )}
            title={t('appLayout.topBar.searchLabel')}
          >
            <Search size={15} strokeWidth={2} aria-hidden="true" />
          </button>
        )}
      </div>

      <div
        className={cn(
          'grid w-full items-center gap-2 @[48rem]/workspace:flex @[48rem]/workspace:w-auto @[48rem]/workspace:flex-shrink-0 @[48rem]/workspace:gap-3',
          showTaskControls ? 'grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)]' : 'grid-cols-1'
        )}
      >
        {showTaskControls && (
          <div
            className="hidden gap-0.5 rounded-button border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.04] @[48rem]/workspace:flex"
            role="group"
            aria-label={t('appLayout.topBar.taskView')}
          >
            {VIEW_OPTIONS.map((opt) => (
              <button
                key={opt.id}
                type="button"
                onClick={() => onViewChange(opt.id)}
                aria-pressed={viewMode === opt.id}
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

        {showTaskControls && (
          <label className="sr-only" htmlFor="topbar-task-view">
            {t('appLayout.topBar.taskView')}
          </label>
        )}
        {showTaskControls && (
          <select
            id="topbar-task-view"
            value={viewMode}
            onChange={(event) => onViewChange(event.target.value as ViewMode)}
            className="h-11 min-w-0 w-full rounded-button border border-black/[0.1] bg-white px-3 text-ui-body font-medium text-foreground-light outline-none transition-colors focus:border-apple-blue focus:ring-2 focus:ring-apple-blue-focus dark:border-white/[0.12] dark:bg-white/[0.06] dark:text-foreground-dark @[48rem]/workspace:hidden"
          >
            {VIEW_OPTIONS.map((opt) => (
              <option key={opt.id} value={opt.id}>
                {t(opt.labelKey)}
              </option>
            ))}
          </select>
        )}

        {showTaskControls && (
          <button
            type="button"
            onClick={onCreateTask}
            aria-label={taskLabel}
            title={taskTitle}
            className={cn(
              'inline-flex min-h-11 w-full min-w-0 items-center justify-start gap-2 rounded-button bg-apple-blue px-3 py-2 text-left text-ui-button font-medium leading-snug text-white transition-colors hover:bg-apple-blue-focus active:scale-[0.99] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus @[48rem]/workspace:col-auto @[48rem]/workspace:w-auto @[48rem]/workspace:max-w-[26rem]',
              'col-start-2'
            )}
          >
            <Plus size={16} strokeWidth={2.25} className="shrink-0" aria-hidden="true" />
            <span className="min-w-0 whitespace-normal break-words">{taskLabel}</span>
          </button>
        )}
      </div>
    </div>
  )
}
