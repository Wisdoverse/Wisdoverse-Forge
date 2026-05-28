import { useState, type FormEvent } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { slugifyName } from '@app/shared/lib/slugify'

interface CreateTeamFormProps {
  onSave: (name: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

const TEAM_SETUP_STEPS = [
  'Name the group people already recognize.',
  'Create the team before adding projects.',
  'Open Team Members after creation to invite people.',
]

export function CreateTeamForm({ onSave, onCancel, saving }: CreateTeamFormProps) {
  const [name, setName] = useState('')
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const nameInputId = 'create-team-name'
  const statusId = 'create-team-status'
  const errorId = 'create-team-name-error'
  const trimmedName = name.trim()
  const isReady = Boolean(trimmedName)
  const visibleError = submitAttempted && !isReady ? 'Enter a team name before creating it.' : null

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    if (!isReady) {
      document.getElementById(nameInputId)?.focus()
      return
    }
    await onSave(trimmedName)
  }

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
          Team setup path
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Use teams for people who share project access and operating responsibility.
        </p>
        <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {TEAM_SETUP_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </div>

      <div className="mb-3">
        <label htmlFor="team-name" className={uiStyles.label}>
          Team Name *
        </label>
        <input
          id="team-name"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Frontend"
          autoFocus
          aria-invalid={visibleError !== null}
          aria-describedby={`${statusId}${visibleError ? ` ${errorId}` : ''}`}
          className={uiStyles.input}
        />
        <p
          id="team-name-help"
          className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          Pick a name teammates will recognize in project lists and access dialogs.
        </p>
        {name.trim() && (
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Slug: {slugifyName(name)}
          </p>
        )}
      </div>
      {visibleError && (
        <p id={errorId} role="alert" className="text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}
      <div className="flex items-center justify-between gap-2">
        <p
          id={statusId}
          data-testid="create-team-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {isReady ? 'Ready to Create Team' : 'Next: Name the Team'}
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
            {saving ? 'Creating…' : 'Create Team'}
          </button>
        </div>
      </div>
    </form>
  )
}
