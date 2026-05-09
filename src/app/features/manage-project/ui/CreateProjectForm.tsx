import { useEffect, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { NavTeam } from '@app/entities/team'
import { slugifyName } from '@app/shared/lib/slugify'

interface CreateProjectFormProps {
  teams: NavTeam[]
  onSave: (name: string, teamId: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

export function CreateProjectForm({ teams, onSave, onCancel, saving }: CreateProjectFormProps) {
  const [name, setName] = useState('')
  const [teamId, setTeamId] = useState(teams[0]?.id ?? '')

  useEffect(() => {
    if (!teamId && teams[0]) {
      setTeamId(teams[0].id)
    }
  }, [teamId, teams])

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!name.trim() || !teamId) return
    await onSave(name.trim(), teamId)
  }

  const inputClass = cn(uiStyles.input)

  return (
    <form
      onSubmit={handleSubmit}
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label className={uiStyles.label}>Project Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Web App"
            autoFocus
            required
            className={inputClass}
          />
          {name.trim() && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Slug: {slugifyName(name)}
            </p>
          )}
        </div>

        <div>
          <label className={uiStyles.label}>Team</label>
          {teams.length === 0 ? (
            <p className="py-1.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              No teams — create a team first
            </p>
          ) : (
            <select
              value={teamId}
              onChange={(e) => setTeamId(e.target.value)}
              required
              className={inputClass}
            >
              {teams.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          )}
        </div>
      </div>

      <div className="flex gap-2 justify-end">
        <button
          type="button"
          onClick={onCancel}
          disabled={saving}
          className={uiStyles.secondaryButton}
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={saving || !name.trim() || !teamId || teams.length === 0}
          className={uiStyles.primaryButton}
        >
          {saving ? 'Creating...' : 'Create Project'}
        </button>
      </div>
    </form>
  )
}
