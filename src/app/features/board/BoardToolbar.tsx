import { LayoutGrid, ListFilter, Search } from 'lucide-react'
import type { ReactNode } from 'react'
import { cn } from '@app/shared/lib/utils'

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

const PRIORITY_FILTERS: { value: BoardPriorityFilter; label: string }[] = [
  { value: 'all', label: 'All Priority' },
  { value: 'urgent', label: 'Urgent' },
  { value: 'high', label: 'High' },
  { value: 'normal', label: 'Normal' },
  { value: 'low', label: 'Low' },
]

const ASSIGNEE_FILTERS: { value: BoardAssigneeFilter; label: string }[] = [
  { value: 'all', label: 'All Owners' },
  { value: 'unassigned', label: 'Unassigned' },
  { value: 'assigned', label: 'Assigned' },
]

const DISPLAY_OPTIONS: { value: BoardDisplayMode; label: string }[] = [
  { value: 'comfortable', label: 'Comfort' },
  { value: 'compact', label: 'Compact' },
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
  const hasActiveFilter =
    searchQuery.trim().length > 0 || priorityFilter !== 'all' || assigneeFilter !== 'all'

  return (
    <section
      data-testid="board-toolbar"
      className="rounded-lg border border-black/[0.08] bg-white px-3 py-2 dark:border-white/[0.1] dark:bg-[#2a2a2c]"
    >
      <div className="flex flex-col gap-2 xl:flex-row xl:items-center xl:justify-between">
        <div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-center">
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
              placeholder="Search tasks, agents, blockers…"
              className="h-9 w-full rounded-lg border border-black/[0.08] bg-black/[0.02] pl-9 pr-3 text-ui-body text-foreground-light outline-none transition-colors placeholder:text-secondary-light focus:border-apple-blue/40 focus:bg-white focus:ring-2 focus:ring-apple-blue/20 dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:placeholder:text-secondary-dark dark:focus:bg-white/[0.06]"
            />
          </label>

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
            ariaLabel="Filter tasks by assignee"
            options={ASSIGNEE_FILTERS.map((filter) => ({
              ...filter,
              count: counts.assignee[filter.value],
            }))}
            value={assigneeFilter}
            onChange={onAssigneeFilterChange}
          />
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-2">
          <span className="text-ui-caption tabular-nums text-secondary-light dark:text-secondary-dark">
            {counts.visible}/{counts.total} tasks
          </span>
          {hasActiveFilter && (
            <button
              type="button"
              onClick={onClear}
              className="inline-flex h-8 items-center justify-center rounded-lg px-2 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35"
            >
              Clear
            </button>
          )}
          <FilterGroup
            ariaLabel="Choose board density"
            icon={<LayoutGrid size={14} strokeWidth={2} aria-hidden="true" />}
            options={DISPLAY_OPTIONS.map((option) => ({ ...option, count: null }))}
            value={displayMode}
            onChange={onDisplayModeChange}
          />
        </div>
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
  options: { value: T; label: string; count: number | null }[]
  value: T
  onChange: (value: T) => void
}) {
  return (
    <div
      className="inline-flex max-w-full items-center gap-1 overflow-x-auto rounded-lg bg-black/[0.035] p-1 dark:bg-white/[0.05]"
      role="group"
      aria-label={ariaLabel}
    >
      {icon && (
        <span className="ml-1 shrink-0 text-secondary-light dark:text-secondary-dark">{icon}</span>
      )}
      {options.map((option) => {
        const selected = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={selected}
            onClick={() => onChange(option.value)}
            className={cn(
              'inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-ui-caption font-medium transition-colors',
              selected
                ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
                : 'text-secondary-light hover:text-foreground-light dark:text-secondary-dark dark:hover:text-foreground-dark'
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
