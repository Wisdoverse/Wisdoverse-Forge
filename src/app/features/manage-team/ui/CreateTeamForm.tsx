import { useRef, useState, type FormEvent } from 'react'
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
  'Open the team after creation to invite people.',
]

function rawTeamCreateError(error: unknown): string {
  if (error instanceof Error) return error.message.trim()
  if (typeof error === 'string') return error.trim()
  if (!error || typeof error !== 'object') return ''

  const value = error as {
    serverError?: unknown
    detail?: unknown
    error?: unknown
    message?: unknown
    reason?: unknown
  }

  for (const candidate of [
    value.serverError,
    value.detail,
    value.error,
    value.message,
    value.reason,
  ]) {
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }

  return ''
}

function teamCreateStatusCode(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const value = error as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      if (typeof candidate === 'number' && Number.isFinite(candidate)) return candidate
      if (typeof candidate === 'string' && /^\d{3}$/.test(candidate.trim())) {
        return Number.parseInt(candidate, 10)
      }
    }
  }

  const match = rawTeamCreateError(error).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function createTeamErrorMessage(error: unknown): string {
  const raw = rawTeamCreateError(error)
  const lower = raw.toLowerCase()
  const code = teamCreateStatusCode(error)

  if (
    code == null &&
    (lower.startsWith('sign in again') ||
      lower.startsWith('ask an owner or admin') ||
      lower.startsWith('open settings') ||
      lower.startsWith('use a different name') ||
      lower.startsWith('enter a team name') ||
      lower.startsWith('wait a minute') ||
      lower.startsWith('check your connection') ||
      lower.startsWith('try to create this team'))
  ) {
    return raw
  }

  if (code === 401 || lower.includes('unauthorized') || lower.includes('sign in again')) {
    return 'Sign in again, then create this team.'
  }
  if (code === 403 || lower.includes('forbidden') || lower.includes('permission')) {
    return 'Ask an owner or admin to let you create teams in this team space.'
  }
  if (code === 404) {
    return 'Open Settings and Teams and Projects again, then create this team. The team space may have changed.'
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return 'Use a different team name, then create this team again.'
  }
  if (code === 422 || lower.includes('validation') || lower.includes('invalid')) {
    return lower.includes('name')
      ? 'Enter a team name, then create this team again.'
      : 'Check the team name, then create this team again.'
  }
  if (code === 429 || lower.includes('rate limit') || lower.includes('too many')) {
    return 'Wait a minute, then create this team again. Too many setup changes are happening right now.'
  }
  if (code != null && code >= 500) {
    return 'Open Settings and Teams and Projects again, then create this team. If it still fails, ask an owner or admin to check Teams and Projects in Settings.'
  }
  if (
    error instanceof TypeError ||
    lower.includes('failed to fetch') ||
    lower.includes('network') ||
    lower.includes('load failed')
  ) {
    return 'Check your connection, then create this team again.'
  }

  return 'Check the team name, then create this team again. Forge could not create the team.'
}

export function CreateTeamForm({ onSave, onCancel, saving }: CreateTeamFormProps) {
  const [name, setName] = useState('')
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const [bannerError, setBannerError] = useState<string | null>(null)
  const formRef = useRef<HTMLFormElement>(null)
  const nameInputId = 'team-name'
  const statusId = 'create-team-status'
  const errorId = 'create-team-name-error'
  const bannerId = 'create-team-banner'
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
    setBannerError(null)
    try {
      await onSave(trimmedName)
    } catch (err) {
      setBannerError(createTeamErrorMessage(err))
      formRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    }
  }

  return (
    <form
      ref={formRef}
      onSubmit={handleSubmit}
      noValidate
      className={cn(
        'border-t border-black/[0.06] p-4 dark:border-white/[0.08]',
        'bg-black/[0.015] dark:bg-white/[0.025]'
      )}
    >
      {bannerError && (
        <div id={bannerId} role="alert" aria-live="polite" className={uiStyles.error}>
          {bannerError}
        </div>
      )}

      <div className="mb-4 border-l-2 border-apple-blue/40 pl-3">
        <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
          Team creation steps
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
        <label htmlFor={nameInputId} className={uiStyles.label}>
          Team name *
        </label>
        <input
          id={nameInputId}
          type="text"
          value={name}
          onChange={(e) => {
            setName(e.target.value)
            if (bannerError) setBannerError(null)
          }}
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
            Team link preview: {slugifyName(name)}. Forge creates it automatically from the team
            name. You do not need to type it.
          </p>
        )}
      </div>
      {visibleError && (
        <p id={errorId} role="alert" aria-live="polite" className="text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}
      <div className="flex items-center justify-between gap-2">
        <p
          id={statusId}
          data-testid="create-team-status"
          className="text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          {isReady ? 'Ready to create team' : 'Next: name the team'}
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
            {saving ? 'Creating…' : 'Create team'}
          </button>
        </div>
      </div>
    </form>
  )
}
