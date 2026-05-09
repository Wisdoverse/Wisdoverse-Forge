import { useEffect, useState } from 'react'
import { BrainCircuit, Plus } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSkillsStore, type Skill } from '@app/shared/model/skills.store'
import { CreateSkillModal } from './CreateSkillModal'
import { SkillCard } from './SkillCard'
import { SkillDetailModal } from './SkillDetailModal'

export function SkillsView() {
  const { loading, error, searchQuery, setSearchQuery, loadSkills, filteredSkills } =
    useSkillsStore()
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null)
  const [createModalOpen, setCreateModalOpen] = useState(false)

  useEffect(() => {
    void loadSkills()
  }, [loadSkills])

  const skills = filteredSkills()

  return (
    <div className="flex h-full flex-col">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center justify-between gap-3 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06] sm:px-6">
        <p className="min-w-0 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {skills.length === 0 ? '' : `${skills.length} skill${skills.length === 1 ? '' : 's'}`}
        </p>
        <div className="flex min-w-0 items-center gap-2">
          <input
            type="search"
            placeholder="Search skills…"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className={cn(uiStyles.input, 'w-36 shrink sm:w-52')}
          />
          <button
            type="button"
            onClick={() => setCreateModalOpen(true)}
            className={uiStyles.primaryButton}
          >
            <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
            <span>New Skill</span>
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {loading && (
          <div className="flex h-full items-center justify-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              Loading skills…
            </p>
          </div>
        )}

        {!loading && error && (
          <div className="flex h-full flex-col items-center justify-center gap-3">
            <p className="text-ui-body text-apple-red">{error}</p>
            <button
              type="button"
              onClick={() => void loadSkills()}
              className={uiStyles.primaryButton}
            >
              Retry
            </button>
          </div>
        )}

        {!loading && !error && skills.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-4 text-center">
            <div className="flex h-14 w-14 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
              <BrainCircuit size={28} strokeWidth={1.75} aria-hidden="true" />
            </div>
            <div className="space-y-1">
              <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
                {searchQuery ? 'No skills match your search' : 'Create your first skill'}
              </p>
              <p className="max-w-sm text-ui-body text-secondary-light dark:text-secondary-dark">
                {searchQuery
                  ? 'Clear the search or add a new skill for this workspace.'
                  : 'Skills store reusable instructions that agents can apply during task work.'}
              </p>
            </div>
            <button
              type="button"
              onClick={() => setCreateModalOpen(true)}
              className={uiStyles.primaryButton}
            >
              <Plus size={14} strokeWidth={2.5} aria-hidden="true" />
              <span>New Skill</span>
            </button>
          </div>
        )}

        {!loading && !error && skills.length > 0 && (
          <div className="flex flex-col gap-2">
            {skills.map((skill) => (
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
