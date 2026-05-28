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

const PROJECT_SETUP_STEPS = [
  'Choose the team that owns the work.',
  'Name the project after the product, repo, or work area.',
  'Open Project Members after creation if access differs from the team.',
]

export function CreateProjectForm({ teams, onSave, onCancel, saving }: CreateProjectFormProps) {
  const [name, setName] = useState('')
  const [teamId, setTeamId] = useState(teams[0]?.id ?? '')
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const nameInputId = 'create-project-name'
  const teamSelectId = 'create-project-team'
  const statusId = 'create-project-status'
  const errorId = 'create-project-error'
  const trimmedName = name.trim()
  const hasTeams = teams.length > 0
  const missingTeam = !hasTeams || !teamId
  const isReady = Boolean(trimmedName) && !missingTeam
  const visibleError =
    submitAttempted && !isReady
      ? missingTeam
        ? 'Create or choose a team before creating this project.'
        : 'Enter a project name before creating it.'
      : null
  const errorField = visibleError === null ? null : missingTeam ? 'team' : 'name'

  useEffect(() => {
    if (!teamId && teams[0]) {
      setTeamId(teams[0].id)
    }
  }, [teamId, teams])

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!isReady) {
      document.getElementById(missingTeam ? teamSelectId : nameInputId)?.focus()
      return
    }
    await onSave(trimmedName, teamId)
  }

  const inputClass = cn(uiStyles.input)

  return (
    <form
      onSubmit={handleSubmit}
      noValidate
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      <div className="mb-4 border-l-2 border-apple-blue/40 pl-3">
        <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
          Project setup path
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Use projects for the work areas where agents receive tasks and evidence.
        </p>
        <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {PROJECT_SETUP_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </div>

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label htmlFor="project-name" className={uiStyles.label}>
            Project Name *
          </label>
          <input
            id="project-name"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Web App"
            autoFocus
            aria-invalid={errorField === 'name'}
            aria-describedby={`${statusId}${errorField === 'name' ? ` ${errorId}` : ''}`}
            className={inputClass}
          />
          <p
            id="project-name-help"
            className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
          >
            Pick the name users will look for when assigning tasks.
          </p>
          {name.trim() && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Slug: {slugifyName(name)}
            </p>
          )}
        </div>

        <div>
          <label htmlFor="project-team" className={uiStyles.label}>
            Team *
          </label>
          {teams.length === 0 ? (
            <p
              id="create-project-team"
              tabIndex={-1}
              className="py-1.5 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              No teams — create a team first
            </p>
          ) : (
            <select
              id="project-team"
              value={teamId}
              onChange={(e) => setTeamId(e.target.value)}
              aria-invalid={errorField === 'team'}
              aria-describedby={`${statusId}${errorField === 'team' ? ` ${errorId}` : ''}`}
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

      {visibleError && errorField === 'name' && (
        <p id={errorId} role="alert" className="text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}
      {visibleError && errorField === 'team' && (
        <p role="alert" className="text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}
      <div className="flex items-center justify-between gap-2">
        <p
          id={statusId}
          data-testid="create-project-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {isReady
            ? 'Ready to Create Project'
            : missingTeam
              ? 'Next: Create a Team First'
              : 'Next: Name the Project'}
        </p>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            className={uiStyles.secondaryButton}
          >
            Cancel
          </button>
          <button type="submit" disabled={saving} className={uiStyles.primaryButton}>
            {saving ? 'Creating…' : 'Create Project'}
          </button>
        </div>
      </div>
    </form>
  )
}
