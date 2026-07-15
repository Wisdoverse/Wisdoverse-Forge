import { useEffect, useId, useMemo, useState } from 'react'
import { BrainCircuit, CheckCircle2, Circle, Filter, Plus, Search, Terminal } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore, type Skill } from '@app/entities/skill'
import { CreateSkillModal } from './CreateSkillModal'
import { SkillCard } from './SkillCard'
import { SkillDetailModal } from './SkillDetailModal'

type SkillFilter = 'all' | 'installed' | 'available' | 'cli'

const SKILL_FILTERS: { value: SkillFilter; label: string; ariaLabel: string }[] = [
  { value: 'all', label: 'All', ariaLabel: 'Show all saved guidance' },
  {
    value: 'installed',
    label: 'Ready to use',
    ariaLabel: 'Show saved guidance that is ready to use',
  },
  {
    value: 'available',
    label: 'Check before use',
    ariaLabel: 'Show saved guidance to check before use',
  },
  {
    value: 'cli',
    label: 'For one work tool',
    ariaLabel: 'Show saved guidance for one work tool',
  },
]

interface SavedInstructionsEmptyState {
  title: string
  detail: string
  action: 'create' | 'reset'
}

const RAW_LOAD_ERROR_PATTERN =
  /\b(?:(?:API|HTTP|Code:)\s*\(?\d{3}|database|sql|stack trace|traceback|exception|panic|internal server error|role required)\b/i

export function SkillsView() {
  const {
    skills: catalogSkills,
    loading,
    error,
    searchQuery,
    setSearchQuery,
    loadSkills,
    filteredSkills,
  } = useSkillsStore()
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null)
  const [createModalOpen, setCreateModalOpen] = useState(false)
  const [skillFilter, setSkillFilter] = useState<SkillFilter>('all')
  const [savedInstructionName, setSavedInstructionName] = useState<string | null>(null)
  const searchHelpId = useId()

  useEffect(() => {
    void loadSkills()
  }, [loadSkills])

  const searchedSkills = filteredSkills()
  const visibleSkills = useMemo(
    () => filterSkills(searchedSkills, skillFilter),
    [searchedSkills, skillFilter]
  )
  const stats = useMemo(() => summarizeSkills(catalogSkills), [catalogSkills])
  const filterCounts = useMemo(
    () => ({
      all: catalogSkills.length,
      installed: stats.installed,
      available: stats.available,
      cli: stats.cliScoped,
    }),
    [catalogSkills.length, stats]
  )
  const hasCatalogSkills = catalogSkills.length > 0
  const emptyState = savedInstructionsEmptyState({
    hasCatalogSkills,
    searchQuery,
    filter: skillFilter,
  })
  const toolbarStatus =
    savedInstructionName && !loading && !error
      ? `Saved "${savedInstructionName}". Open it to check or reuse it on a task.`
      : skillToolbarStatus({
          visibleCount: visibleSkills.length,
          totalCount: catalogSkills.length,
          searchQuery,
          filter: skillFilter,
          loading,
          error,
        })
  const updateSearchQuery = (query: string) => {
    setSavedInstructionName(null)
    setSearchQuery(query)
  }
  const updateSkillFilter = (filter: SkillFilter) => {
    setSavedInstructionName(null)
    setSkillFilter(filter)
  }
  const resetSkillView = () => {
    updateSearchQuery('')
    setSkillFilter('all')
  }

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center justify-between gap-3 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06] sm:px-6">
        <p
          aria-live="polite"
          className="min-w-0 truncate text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {toolbarStatus}
        </p>
        <div className="flex min-w-0 items-center gap-2">
          <label htmlFor="skill-search" className="sr-only">
            Search saved guidance
          </label>
          <div className="relative">
            <Search
              size={15}
              strokeWidth={2}
              className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
              aria-hidden="true"
            />
            <input
              id="skill-search"
              type="search"
              aria-describedby={searchHelpId}
              placeholder="Search saved guidance..."
              value={searchQuery}
              onChange={(e) => updateSearchQuery(e.target.value)}
              className={cn(uiStyles.input, 'w-40 shrink pl-9 sm:w-56')}
            />
          </div>
          <p
            id={searchHelpId}
            className="hidden max-w-[14rem] text-ui-caption text-secondary-light dark:text-secondary-dark lg:block"
          >
            Search only narrows this list. Use Show all saved guidance to return to the full list.
          </p>
          <button
            type="button"
            onClick={() => setCreateModalOpen(true)}
            className={uiStyles.primaryButton}
          >
            <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
            <span>Save guidance</span>
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {!loading && !error && hasCatalogSkills && (
          <section
            data-testid="skill-reuse-summary"
            className="mb-4 grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(260px,0.7fr)]"
          >
            <div className="grid gap-2 sm:grid-cols-4">
              <SkillStat label="Total" value={stats.total} Icon={BrainCircuit} />
              <SkillStat label="Ready to use" value={stats.installed} Icon={CheckCircle2} />
              <SkillStat label="Check before use" value={stats.available} Icon={Circle} />
              <SkillStat label="For one work tool" value={stats.cliScoped} Icon={Terminal} />
            </div>
            <div className={cn(uiStyles.card, 'p-3')}>
              <div className="mb-2 flex items-center gap-2 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                <Filter size={14} strokeWidth={2.25} aria-hidden="true" />
                <span>Show saved guidance</span>
              </div>
              <div
                role="group"
                aria-label="Saved guidance view choices"
                className="flex flex-wrap gap-1.5"
              >
                {SKILL_FILTERS.map((filter) => (
                  <SkillFilterButton
                    key={filter.value}
                    active={skillFilter === filter.value}
                    label={filter.label}
                    ariaLabel={filter.ariaLabel}
                    count={filterCounts[filter.value]}
                    onClick={() => updateSkillFilter(filter.value)}
                  />
                ))}
              </div>
            </div>
          </section>
        )}

        {loading && (
          <div className="flex h-full items-center justify-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Checking saved guidance...
            </p>
          </div>
        )}

        {!loading && error && (
          <div
            role="alert"
            aria-live="polite"
            className="flex h-full flex-col items-center justify-center gap-3 text-center"
          >
            <div className="space-y-1">
              <p className="text-ui-body text-apple-red">
                {savedInstructionsLoadErrorMessage(error)}
              </p>
              <p className="max-w-sm text-ui-caption text-secondary-light dark:text-secondary-dark">
                {savedInstructionsLoadRecoveryMessage(error)}
              </p>
            </div>
            <button
              type="button"
              onClick={() => void loadSkills()}
              className={uiStyles.primaryButton}
            >
              Check saved guidance again
            </button>
          </div>
        )}

        {!loading && !error && visibleSkills.length === 0 && (
          <div
            role="status"
            aria-live="polite"
            data-testid="saved-instructions-empty-state"
            className="flex h-full flex-col items-center justify-center gap-4 text-center"
          >
            <div className="flex h-14 w-14 items-center justify-center rounded-card bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
              <BrainCircuit size={28} strokeWidth={1.75} aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                {emptyState.title}
              </p>
              <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
                {emptyState.detail}
              </p>
            </div>
            <button
              type="button"
              onClick={
                emptyState.action === 'reset' ? resetSkillView : () => setCreateModalOpen(true)
              }
              className={uiStyles.primaryButton}
            >
              {emptyState.action === 'create' && (
                <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
              )}
              <span>
                {emptyState.action === 'reset' ? 'Show all saved guidance' : 'Save guidance'}
              </span>
            </button>
          </div>
        )}

        {!loading && !error && visibleSkills.length > 0 && (
          <div className="flex flex-col gap-2">
            {visibleSkills.map((skill) => (
              <SkillCard
                key={`${skill.plugin}/${skill.name}`}
                skill={skill}
                onClick={setSelectedSkill}
              />
            ))}
          </div>
        )}
      </div>

      {/* Detail modal */}
      {selectedSkill && (
        <SkillDetailModal skill={selectedSkill} onClose={() => setSelectedSkill(null)} />
      )}
      <CreateSkillModal
        open={createModalOpen}
        onClose={() => setCreateModalOpen(false)}
        onCreated={(skill) => setSavedInstructionName(skill.name)}
      />
    </div>
  )
}

function savedInstructionsLoadErrorMessage(error: string): string {
  return RAW_LOAD_ERROR_PATTERN.test(error) ? 'Saved guidance needs to load again.' : error
}

function savedInstructionsLoadRecoveryMessage(error: string): string {
  const normalized = error.toLowerCase()
  if (normalized.includes('sign in')) {
    return 'After signing in, choose Check saved guidance again.'
  }
  if (
    normalized.includes('permission') ||
    normalized.includes('access') ||
    normalized.includes('role required')
  ) {
    return 'After an owner or admin updates your access, choose Check saved guidance again.'
  }
  if (normalized.includes('connect') || normalized.includes('connection')) {
    return 'Check your connection, then choose Check saved guidance again.'
  }
  return 'Choose Check saved guidance again to load the list.'
}

function savedInstructionsEmptyState({
  hasCatalogSkills,
  searchQuery,
  filter,
}: {
  hasCatalogSkills: boolean
  searchQuery: string
  filter: SkillFilter
}): SavedInstructionsEmptyState {
  const hasSearch = searchQuery.trim().length > 0
  const hasFilter = filter !== 'all'

  if (hasCatalogSkills && hasSearch && hasFilter) {
    return {
      title: 'Nothing matches this saved guidance view',
      detail: 'Use Show all saved guidance before assuming nothing useful is saved.',
      action: 'reset',
    }
  }

  if (hasCatalogSkills && hasSearch) {
    return {
      title: 'Nothing matches your saved guidance search',
      detail: 'Use Show all saved guidance to return to the full list.',
      action: 'reset',
    }
  }

  if (hasCatalogSkills && hasFilter) {
    return {
      title: 'Nothing matches this saved guidance view',
      detail: 'Use Show all saved guidance to return to the full list.',
      action: 'reset',
    }
  }

  if (hasSearch) {
    return {
      title: 'No saved guidance matches that search yet',
      detail: 'If this is reusable guidance your team needs, choose Save guidance and add it now.',
      action: 'create',
    }
  }

  return {
    title: 'Create your first saved guidance',
    detail:
      'Save steps your agents should repeat, like checking work before sharing it or writing a short update.',
    action: 'create',
  }
}

function filterSkills(skills: Skill[], filter: SkillFilter): Skill[] {
  switch (filter) {
    case 'installed':
      return skills.filter((skill) => skill.installed)
    case 'available':
      return skills.filter((skill) => !skill.installed)
    case 'cli':
      return skills.filter((skill) => skill.cliTool)
    case 'all':
    default:
      return skills
  }
}

function summarizeSkills(skills: Skill[]) {
  return skills.reduce(
    (summary, skill) => {
      summary.total += 1
      if (skill.installed) summary.installed += 1
      else summary.available += 1
      if (skill.cliTool) summary.cliScoped += 1
      return summary
    },
    { total: 0, installed: 0, available: 0, cliScoped: 0 }
  )
}

function skillToolbarStatus({
  visibleCount,
  totalCount,
  searchQuery,
  filter,
  loading,
  error,
}: {
  visibleCount: number
  totalCount: number
  searchQuery: string
  filter: SkillFilter
  loading: boolean
  error: string | null
}) {
  if (loading) return 'Checking saved guidance'
  if (error) return 'Check saved guidance again to continue.'
  if (visibleCount > 0) {
    return `${visibleCount} saved guidance item${visibleCount === 1 ? '' : 's'}`
  }
  if (totalCount === 0) return 'Choose Save guidance to start.'
  if (searchQuery.trim() && filter !== 'all') return 'Nothing matches this saved guidance view.'
  if (searchQuery.trim()) return 'Nothing matches your saved guidance search.'
  if (filter !== 'all') return 'Nothing matches this saved guidance view.'
  return 'Choose Save guidance to add reusable guidance.'
}

function SkillStat({
  label,
  value,
  Icon,
}: {
  label: string
  value: number
  Icon: typeof BrainCircuit
}) {
  return (
    <div className={cn(uiStyles.card, 'flex min-w-0 items-center gap-3 px-3 py-3')}>
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-card bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
        <Icon size={16} strokeWidth={2.1} aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block text-ui-metric font-semibold tabular-nums text-foreground-light dark:text-foreground-dark">
          {value}
        </span>
        <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {label}
        </span>
      </span>
    </div>
  )
}

function SkillFilterButton({
  active,
  label,
  ariaLabel,
  count,
  onClick,
}: {
  active: boolean
  label: string
  ariaLabel: string
  count: number
  onClick: () => void
}) {
  const countLabel = `${count} matching saved guidance item${count === 1 ? '' : 's'}`

  return (
    <button
      type="button"
      aria-pressed={active}
      aria-label={`${ariaLabel}, ${countLabel}`}
      onClick={onClick}
      className={cn(
        'inline-flex h-8 min-w-0 items-center gap-1.5 rounded-button border px-2.5 text-ui-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
        active
          ? 'border-black/[0.08] bg-black/[0.06] text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.08] dark:text-foreground-dark'
          : 'border-black/[0.08] bg-white text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
      )}
    >
      <span className="truncate">{label}</span>
      <span className="tabular-nums opacity-70" aria-hidden="true">
        {count}
      </span>
    </button>
  )
}
