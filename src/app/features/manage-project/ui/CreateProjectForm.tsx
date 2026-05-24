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
  const readinessTitle = missingTeam
    ? 'Next: Create a Team First'
    : trimmedName
      ? 'Ready to Create Project'
      : 'Next: Name the Project'
  const readinessDetail = missingTeam
    ? 'Every project belongs to a team so access and ownership stay clear.'
    : trimmedName
      ? 'Create this project, then add agents and tasks to it.'
      : 'Use a name people recognize, such as the product, repo, or customer.'

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
      <div
        id={statusId}
        data-testid="create-project-status"
        aria-live="polite"
        className={cn(
          'mb-3 rounded-card border px-3 py-2',
          isReady
            ? 'border-apple-green/25 bg-apple-green/10'
            : 'border-apple-blue/20 bg-apple-blue/[0.04]'
        )}
      >
        <p className="text-ui-button font-semibold text-foreground-light dark:text-foreground-dark">
          {readinessTitle}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {readinessDetail}
        </p>
      </div>

      {visibleError && (
        <div className={cn(uiStyles.error, 'mb-3')} role="alert" aria-live="polite">
          {visibleError}
        </div>
      )}

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label htmlFor={nameInputId} className={uiStyles.label}>
            Project Name
          </label>
          <input
            id={nameInputId}
            name="projectName"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Web App"
            autoFocus
            aria-invalid={errorField === 'name'}
            aria-describedby={`${statusId}${errorField === 'name' ? ` ${errorId}` : ''}`}
            className={inputClass}
          />
          {errorField === 'name' && (
            <p id={errorId} className="mt-1 text-ui-caption text-apple-red">
              {visibleError}
            </p>
          )}
          {trimmedName && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Slug: {slugifyName(name)}
            </p>
          )}
        </div>

        <div>
          <label htmlFor={hasTeams ? teamSelectId : undefined} className={uiStyles.label}>
            Team
          </label>
          {!hasTeams ? (
            <p
              id={teamSelectId}
              tabIndex={-1}
              aria-describedby={errorField === 'team' ? errorId : undefined}
              className="py-1.5 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              No teams — create a team first
            </p>
          ) : (
            <select
              id={teamSelectId}
              name="teamId"
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
          {errorField === 'team' && (
            <p id={errorId} className="mt-1 text-ui-caption text-apple-red">
              {visibleError}
            </p>
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
        <button type="submit" disabled={saving} className={uiStyles.primaryButton}>
          {saving ? 'Creating…' : 'Create Project'}
        </button>
      </div>
    </form>
  )
}
