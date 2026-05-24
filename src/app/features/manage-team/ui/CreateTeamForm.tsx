import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { slugifyName } from '@app/shared/lib/slugify'

interface CreateTeamFormProps {
  onSave: (name: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

export function CreateTeamForm({ onSave, onCancel, saving }: CreateTeamFormProps) {
  const [name, setName] = useState('')
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const nameInputId = 'create-team-name'
  const statusId = 'create-team-status'
  const errorId = 'create-team-name-error'
  const trimmedName = name.trim()
  const isReady = Boolean(trimmedName)
  const visibleError = submitAttempted && !isReady ? 'Enter a team name before creating it.' : null

  async function handleSubmit(e: React.FormEvent) {
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
      <div
        id={statusId}
        data-testid="create-team-status"
        aria-live="polite"
        className={cn(
          'mb-3 rounded-card border px-3 py-2',
          isReady
            ? 'border-apple-green/25 bg-apple-green/10'
            : 'border-apple-blue/20 bg-apple-blue/[0.04]'
        )}
      >
        <p className="text-ui-button font-semibold text-foreground-light dark:text-foreground-dark">
          {isReady ? 'Ready to Create Team' : 'Next: Name the Team'}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {isReady
            ? 'Create this team, then add its projects and members.'
            : 'Use a short name people recognize, such as Frontend or Platform.'}
        </p>
      </div>

      {visibleError && (
        <div className={cn(uiStyles.error, 'mb-3')} role="alert" aria-live="polite">
          {visibleError}
        </div>
      )}

      <div className="mb-3">
        <label htmlFor={nameInputId} className={uiStyles.label}>
          Team Name
        </label>
        <input
          id={nameInputId}
          name="teamName"
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="e.g. Frontend"
          autoFocus
          aria-invalid={visibleError !== null}
          aria-describedby={`${statusId}${visibleError ? ` ${errorId}` : ''}`}
          className={uiStyles.input}
        />
        {visibleError && (
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
          {saving ? 'Creating…' : 'Create Team'}
        </button>
      </div>
    </form>
  )
}
