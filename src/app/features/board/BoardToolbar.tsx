import { LayoutGrid, ListFilter, Search } from 'lucide-react'
import { useId, useState, type ReactNode } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'

export type BoardPriorityFilter = 'all' | 'urgent' | 'high' | 'normal' | 'low'
export type BoardAssigneeFilter = 'all' | 'assigned' | 'unassigned'
export type BoardDisplayMode = 'comfortable' | 'compact'

export interface BoardFilterCounts {
  total: number
  visible: number
  priority: Record<BoardPriorityFilter, number>
  assignee: Record<BoardAssigneeFilter, number>
}

interface BoardToolbarProps {
  searchQuery: string
  onSearchQueryChange: (value: string) => void
  priorityFilter: BoardPriorityFilter
  onPriorityFilterChange: (value: BoardPriorityFilter) => void
  assigneeFilter: BoardAssigneeFilter
  onAssigneeFilterChange: (value: BoardAssigneeFilter) => void
  displayMode: BoardDisplayMode
  onDisplayModeChange: (value: BoardDisplayMode) => void
  counts: BoardFilterCounts
  onClear: () => void
}

const PRIORITY_FILTERS: { value: BoardPriorityFilter; label: string; ariaLabel: string }[] = [
  { value: 'all', label: 'All priorities', ariaLabel: 'Show tasks at all priority levels' },
  { value: 'urgent', label: 'Urgent', ariaLabel: 'Show urgent priority tasks' },
  { value: 'high', label: 'High', ariaLabel: 'Show high priority tasks' },
  { value: 'normal', label: 'Normal', ariaLabel: 'Show normal priority tasks' },
  { value: 'low', label: 'Low', ariaLabel: 'Show low priority tasks' },
]

const ASSIGNEE_FILTERS: { value: BoardAssigneeFilter; label: string; ariaLabel: string }[] = [
  { value: 'all', label: 'All agents', ariaLabel: 'Show tasks for all agent choices' },
  { value: 'unassigned', label: 'Needs agent', ariaLabel: 'Show tasks that still need an agent' },
  { value: 'assigned', label: 'Has agent', ariaLabel: 'Show tasks that already have an agent' },
]

const DISPLAY_OPTIONS: { value: BoardDisplayMode; label: string; ariaLabel: string }[] = [
  { value: 'comfortable', label: 'Guided', ariaLabel: 'Use guided task cards' },
  { value: 'compact', label: 'Compact', ariaLabel: 'Use compact task cards' },
]

export function BoardToolbar({
  searchQuery,
  onSearchQueryChange,
  priorityFilter,
  onPriorityFilterChange,
  assigneeFilter,
  onAssigneeFilterChange,
  displayMode,
  onDisplayModeChange,
  counts,
  onClear,
}: BoardToolbarProps) {
  const searchHelpId = useId()
  const filtersPanelId = useId()
  const [filtersOpen, setFiltersOpen] = useState(false)
  const hasActiveFilter =
    searchQuery.trim().length > 0 || priorityFilter !== 'all' || assigneeFilter !== 'all'
  const advancedFilterCount = Number(priorityFilter !== 'all') + Number(assigneeFilter !== 'all')
  const filtersButtonLabel =
    advancedFilterCount > 0 ? `Filters (${advancedFilterCount} active)` : 'Filters'

  return (
    <section data-testid="board-toolbar" className={cn(uiStyles.card, 'px-3 py-2')}>
      <div className="flex flex-col gap-2">
        <div className="flex flex-col gap-2 xl:flex-row xl:items-center xl:justify-between">
          <div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-start">
            <label className="relative min-w-0 flex-1 sm:w-72 sm:flex-none">
              <span className="sr-only">Search tasks</span>
              <Search
                size={15}
                strokeWidth={2}
                className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
                aria-hidden="true"
              />
              <input
                data-testid="board-search"
                type="search"
                value={searchQuery}
                onChange={(event) => onSearchQueryChange(event.target.value)}
                aria-describedby={searchHelpId}
                placeholder="Search task names, agents, or help needed..."
                className={cn(
                  uiStyles.input,
                  'bg-black/[0.02] pl-9 pr-3 focus:bg-white dark:focus:bg-white/[0.06]'
                )}
              />
              <span
                id={searchHelpId}
                className="mt-1 block text-ui-caption text-secondary-light dark:text-secondary-dark"
              >
                Search only narrows tasks shown below. Use Show all tasks to return to the full
                board.
              </span>
            </label>
            <button
              type="button"
              aria-expanded={filtersOpen}
              aria-controls={filtersPanelId}
              onClick={() => setFiltersOpen((open) => !open)}
              className={cn(
                uiStyles.secondaryButton,
                'shrink-0',
                filtersOpen || advancedFilterCount > 0
                  ? 'bg-black/[0.06] dark:bg-white/[0.1]'
                  : 'bg-black/[0.02] dark:bg-white/[0.04]'
              )}
            >
              <ListFilter size={15} strokeWidth={2} aria-hidden="true" />
              <span>{filtersButtonLabel}</span>
            </button>
          </div>

          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <span
              role="status"
              aria-live="polite"
              className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark"
            >
              Showing {counts.visible} of {counts.total} tasks
            </span>
            {hasActiveFilter && (
              <button type="button" onClick={onClear} className={uiStyles.subtleButton}>
                Show all tasks
              </button>
            )}
            <FilterGroup
              ariaLabel="Choose card detail level"
              icon={<LayoutGrid size={14} strokeWidth={2} aria-hidden="true" />}
              options={DISPLAY_OPTIONS.map((option) => ({ ...option, count: null }))}
              value={displayMode}
              onChange={onDisplayModeChange}
            />
          </div>
        </div>
        {filtersOpen ? (
          <div id={filtersPanelId} className="flex flex-wrap items-center gap-2">
            <FilterGroup
              ariaLabel="Filter tasks by priority"
              icon={<ListFilter size={14} strokeWidth={2} aria-hidden="true" />}
              options={PRIORITY_FILTERS.map((filter) => ({
                ...filter,
                count: counts.priority[filter.value],
              }))}
              value={priorityFilter}
              onChange={onPriorityFilterChange}
            />

            <FilterGroup
              ariaLabel="Filter tasks by whether an agent is chosen"
              options={ASSIGNEE_FILTERS.map((filter) => ({
                ...filter,
                count: counts.assignee[filter.value],
              }))}
              value={assigneeFilter}
              onChange={onAssigneeFilterChange}
            />
          </div>
        ) : null}
      </div>
    </section>
  )
}

function FilterGroup<T extends string>({
  ariaLabel,
  icon,
  options,
  value,
  onChange,
}: {
  ariaLabel: string
  icon?: ReactNode
  options: { value: T; label: string; ariaLabel: string; count: number | null }[]
  value: T
  onChange: (value: T) => void
}) {
  return (
    <div
      className="inline-flex max-w-full items-center gap-1 overflow-x-auto rounded-button bg-black/[0.035] p-1 dark:bg-white/[0.05]"
      role="group"
      aria-label={ariaLabel}
    >
      {icon && (
        <span className="ml-1 shrink-0 text-secondary-light dark:text-secondary-dark">{icon}</span>
      )}
      {options.map((option) => {
        const selected = option.value === value
        const countLabel =
          typeof option.count === 'number'
            ? `${option.count} matching ${option.count === 1 ? 'task' : 'tasks'}`
            : null
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={selected}
            aria-label={countLabel ? `${option.ariaLabel}, ${countLabel}` : option.ariaLabel}
            onClick={() => onChange(option.value)}
            className={cn(
              'inline-flex h-7 shrink-0 items-center gap-1 rounded-button px-2 text-ui-caption font-medium transition-colors',
              selected
                ? 'bg-black/[0.06] text-foreground-light dark:bg-white/[0.1] dark:text-foreground-dark'
                : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
            )}
          >
            <span>{option.label}</span>
            {typeof option.count === 'number' && (
              <span className="tabular-nums text-secondary-light dark:text-secondary-dark">
                {option.count}
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}
