import { useEffect, useMemo, useState } from 'react'
import { BrainCircuit, CheckCircle2, Circle, Filter, Plus, Search, Terminal } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore, type Skill } from '@app/shared/model/skills.store'
import { CreateSkillModal } from './CreateSkillModal'
import { SkillCard } from './SkillCard'
import { SkillDetailModal } from './SkillDetailModal'

type SkillFilter = 'all' | 'installed' | 'available' | 'cli'

const SKILL_FILTER_LABELS: Record<SkillFilter, string> = {
  all: 'All',
  installed: 'Installed',
  available: 'Available',
  cli: 'For one work tool',
}

const RAW_LOAD_ERROR_PATTERN = /\b(?:API|HTTP|Code:)\s*\(?\d{3}\b/i

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
  const toolbarStatus = skillToolbarStatus({
    visibleCount: visibleSkills.length,
    totalCount: catalogSkills.length,
    searchQuery,
    filter: skillFilter,
    loading,
    error,
  })

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
            Search saved instructions
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
              placeholder="Search saved instructions..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className={cn(uiStyles.input, 'w-40 shrink pl-9 sm:w-56')}
            />
          </div>
          <button
            type="button"
            onClick={() => setCreateModalOpen(true)}
            className={uiStyles.primaryButton}
          >
            <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
            <span>New Instruction</span>
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
              <SkillStat label="Installed" value={stats.installed} Icon={CheckCircle2} />
              <SkillStat label="Available" value={stats.available} Icon={Circle} />
              <SkillStat label="For one work tool" value={stats.cliScoped} Icon={Terminal} />
            </div>
            <div className="rounded-card border border-black/[0.08] bg-white p-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
              <div className="mb-2 flex items-center gap-2 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                <Filter size={14} strokeWidth={2.25} aria-hidden="true" />
                <span>Show saved instructions</span>
              </div>
              <div
                role="group"
                aria-label="Saved instruction filter"
                className="flex flex-wrap gap-1.5"
              >
                {(Object.keys(SKILL_FILTER_LABELS) as SkillFilter[]).map((filter) => (
                  <SkillFilterButton
                    key={filter}
                    active={skillFilter === filter}
                    label={SKILL_FILTER_LABELS[filter]}
                    count={filterCounts[filter]}
                    onClick={() => setSkillFilter(filter)}
                  />
                ))}
              </div>
            </div>
          </section>
        )}

        {loading && (
          <div className="flex h-full items-center justify-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading saved instructions...
            </p>
          </div>
        )}

        {!loading && error && (
          <div
            role="alert"
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
              Retry
            </button>
          </div>
        )}

        {!loading && !error && visibleSkills.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-4 text-center">
            <div className="flex h-14 w-14 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
              <BrainCircuit size={28} strokeWidth={1.75} aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                {hasCatalogSkills
                  ? 'No saved instructions match this view'
                  : searchQuery
                    ? 'No saved instructions match your search'
                    : 'Create your first saved instruction'}
              </p>
              <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
                {hasCatalogSkills
                  ? 'Adjust search or filters to review reusable instructions.'
                  : searchQuery
                    ? 'Clear the search or add a new saved instruction for this workspace.'
                    : 'Saved instructions are reusable steps that agents can apply during task work.'}
              </p>
            </div>
            <button
              type="button"
              onClick={() => setCreateModalOpen(true)}
              className={uiStyles.primaryButton}
            >
              <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
              <span>New Instruction</span>
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
      <CreateSkillModal open={createModalOpen} onClose={() => setCreateModalOpen(false)} />
    </div>
  )
}

function savedInstructionsLoadErrorMessage(error: string): string {
  return RAW_LOAD_ERROR_PATTERN.test(error) ? 'Saved instructions could not load.' : error
}

function savedInstructionsLoadRecoveryMessage(error: string): string {
  const normalized = error.toLowerCase()
  if (normalized.includes('sign in')) return 'After signing in, choose Retry.'
  if (normalized.includes('permission') || normalized.includes('access')) {
    return 'After an owner or admin updates your access, choose Retry.'
  }
  if (normalized.includes('connect') || normalized.includes('connection')) {
    return 'Check your connection, then choose Retry.'
  }
  return 'Choose Retry to refresh Saved instructions.'
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
  if (loading) return 'Checking saved instructions'
  if (error) return 'Saved instructions need attention'
  if (visibleCount > 0) {
    return `${visibleCount} saved instruction${visibleCount === 1 ? '' : 's'}`
  }
  if (totalCount === 0) return 'No saved instructions yet'
  if (searchQuery.trim()) return 'No saved instructions match search'
  if (filter !== 'all') return 'No saved instructions match filter'
  return 'No saved instructions to show'
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
    <div className="flex min-w-0 items-center gap-3 rounded-card border border-black/[0.08] bg-white px-3 py-3 dark:border-white/[0.1] dark:bg-[#2a2a2c]">
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
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
  count,
  onClick,
}: {
  active: boolean
  label: string
  count: number
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        'inline-flex h-7 min-w-0 items-center gap-1.5 rounded-full border px-2.5 text-ui-caption font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
        active
          ? 'border-apple-blue/45 bg-apple-blue/[0.08] text-apple-blue'
          : 'border-black/[0.08] bg-white text-secondary-light hover:border-apple-blue/30 hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:text-foreground-dark'
      )}
    >
      <span className="truncate">{label}</span>
      <span className="tabular-nums opacity-70">{count}</span>
    </button>
  )
}
