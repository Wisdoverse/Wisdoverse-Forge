import { useEffect, useRef, useState, type FormEvent } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import type { NavTeam } from '@app/entities/team'
import { slugifyName } from '@app/shared/lib/slugify'

interface CreateProjectFormProps {
  teams: NavTeam[]
  onSave: (name: string, teamId: string, repositoryUrl?: string) => Promise<void>
  onCancel: () => void
  saving: boolean
}

const PROJECT_SETUP_STEPS = [
  'Choose the team that owns the work.',
  'Name the project after the product, app, or work area.',
  'Optional: paste an https:// code link. Use SSH code access in Settings for links that start with git@.',
]

/**
 * Validate the optional repository URL in the SUBMIT HANDLER (not via
 * `register(..., { required })`, which this codebase's modals never render —
 * the #594/#595 silent-dead-click bug class). Returns a user-facing error
 * string, or `null` when the value is acceptable (including empty, since the
 * field is optional). Mirrors the server's parse-time rules so the user gets a
 * fast, local error before the round trip: HTTPS only, no embedded credentials.
 */
export function validateRepositoryUrl(raw: string): string | null {
  const value = raw.trim()
  if (!value) return null // optional — empty is valid
  if (/^(?:git@|ssh:\/\/)/i.test(value)) {
    return 'Use a code link that starts with https://. Links that start with git@ go in SSH code access.'
  }

  let parsed: URL
  try {
    parsed = new URL(value)
  } catch {
    return 'Enter a valid code link, e.g. https://github.com/org/repo.git'
  }
  if (parsed.protocol !== 'https:') {
    return 'Use a code link that starts with https://. Links that start with git@ go in SSH code access.'
  }
  // No credentials embedded in the URL (user[:pass]@host) — the server rejects
  // these so a token never lands in a stored URL. `URL` also flags a bare `@`.
  if (parsed.username || parsed.password || value.includes('@')) {
    return 'Remove account details from the code link. Connect code access in Settings instead.'
  }
  return null
}

function rawProjectCreateError(error: unknown): string {
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

function projectCreateStatusCode(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const value = error as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      if (typeof candidate === 'number' && Number.isFinite(candidate)) return candidate
      if (typeof candidate === 'string' && /^\d{3}$/.test(candidate.trim())) {
        return Number.parseInt(candidate, 10)
      }
    }
  }

  const match = rawProjectCreateError(error).match(
    /\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i
  )
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function createProjectErrorMessage(error: unknown): string {
  const raw = rawProjectCreateError(error)
  const lower = raw.toLowerCase()
  const code = projectCreateStatusCode(error)

  if (code === 401 || lower.includes('unauthorized') || lower.includes('sign in again')) {
    return 'Sign in again, then create this project.'
  }
  if (code === 403 || lower.includes('forbidden') || lower.includes('permission')) {
    return 'Ask an owner or admin to let you create projects in this team.'
  }
  if (code === 404) {
    return 'Refresh Settings, choose the team again, then create this project.'
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return 'Choose a different project name, then create this project again.'
  }
  if (
    lower.includes('repository_url') ||
    lower.includes('repository url') ||
    lower.includes('repo url') ||
    lower.includes('https url')
  ) {
    return 'Use an https:// code link without account details, or leave the code link blank.'
  }
  if (
    lower.includes('credential') ||
    lower.includes('token') ||
    lower.includes('password') ||
    lower.includes('username')
  ) {
    return 'Remove account details from the code link. Connect code access in Settings instead.'
  }
  if (code === 422 || lower.includes('validation') || lower.includes('invalid')) {
    return 'Check the project name, team, and code link, then create this project again.'
  }
  if (code === 429 || lower.includes('rate limit') || lower.includes('too many')) {
    return 'Wait a minute, then create this project again. Too many project changes are happening right now.'
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then create this project again. Forge could not create the project right now. If it still fails, ask an owner or admin to check project setup.'
  }
  if (
    error instanceof TypeError ||
    lower.includes('failed to fetch') ||
    lower.includes('network') ||
    lower.includes('load failed')
  ) {
    return 'Check your connection, then create this project again.'
  }

  return 'Check the project name and team, then create this project again. Forge could not create the project.'
}

export function CreateProjectForm({ teams, onSave, onCancel, saving }: CreateProjectFormProps) {
  const [name, setName] = useState('')
  const [teamId, setTeamId] = useState(teams[0]?.id ?? '')
  const [repositoryUrl, setRepositoryUrl] = useState('')
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const [bannerError, setBannerError] = useState<string | null>(null)
  const formRef = useRef<HTMLFormElement>(null)
  const nameInputId = 'create-project-name'
  const teamSelectId = 'create-project-team'
  const repoInputId = 'create-project-repo'
  const statusId = 'create-project-status'
  const errorId = 'create-project-error'
  const bannerId = 'create-project-banner'
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
  // Derive the read-only workspace path the same way the backend slugifies the
  // name. The user never types a host path; this is a non-editable preview.
  const workspacePath = trimmedName ? `/workspace/${slugifyName(name)}` : null

  useEffect(() => {
    if (!teamId && teams[0]) {
      setTeamId(teams[0].id)
    }
  }, [teamId, teams])

  function focusTop() {
    formRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    setBannerError(null)

    if (!isReady) {
      document.getElementById(missingTeam ? teamSelectId : nameInputId)?.focus()
      return
    }

    // Validate the optional repo URL in the handler so an invalid value shows a
    // banner + blocks submit (no silent dead-click), instead of relying on
    // formState.errors the modal never renders.
    const repoError = validateRepositoryUrl(repositoryUrl)
    if (repoError) {
      setBannerError(repoError)
      focusTop()
      document.getElementById(repoInputId)?.focus()
      return
    }

    const trimmedRepo = repositoryUrl.trim()
    try {
      await onSave(trimmedName, teamId, trimmedRepo || undefined)
    } catch (err) {
      // Surface the server's rejection (e.g. invalid URL / embedded creds the
      // local check did not catch) as a banner rather than failing silently.
      setBannerError(createProjectErrorMessage(err))
      focusTop()
    }
  }

  const inputClass = cn(uiStyles.input)

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
        <div id={bannerId} role="alert" className={uiStyles.error}>
          {bannerError}
        </div>
      )}

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
          <label htmlFor={nameInputId} className={uiStyles.label}>
            Project name *
          </label>
          <input
            id={nameInputId}
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
          {trimmedName && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Address preview: {slugifyName(name)}. Forge creates this automatically from the name.
            </p>
          )}
        </div>

        <div>
          <label htmlFor={teamSelectId} className={uiStyles.label}>
            Team *
          </label>
          {teams.length === 0 ? (
            <p
              id={teamSelectId}
              tabIndex={-1}
              className="py-1.5 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              No teams — create a team first
            </p>
          ) : (
            <select
              id={teamSelectId}
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

      <div className="mb-3">
        <label htmlFor={repoInputId} className={uiStyles.label}>
          Code link
          <span className="ml-1 font-normal text-secondary-light dark:text-secondary-dark">
            (optional)
          </span>
        </label>
        <input
          id={repoInputId}
          type="url"
          inputMode="url"
          value={repositoryUrl}
          onChange={(e) => {
            setRepositoryUrl(e.target.value)
            if (bannerError) setBannerError(null)
          }}
          placeholder="https://github.com/org/repo.git"
          aria-describedby="project-repo-help"
          className={inputClass}
        />
        <p
          id="project-repo-help"
          className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
        >
          Optional — paste a GitHub or GitLab https:// link. Forge copies that code into this
          project. If your link starts with git@, add it in SSH code access first.
        </p>
        {workspacePath && (
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Work folder preview:{' '}
            <span className="font-mono text-[11px] text-foreground-light dark:text-foreground-dark">
              {workspacePath}
            </span>
          </p>
        )}
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
            ? 'Ready to create project'
            : missingTeam
              ? 'Next: create a team first'
              : 'Next: name the project'}
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
            {saving ? 'Creating…' : 'Create project'}
          </button>
        </div>
      </div>
    </form>
  )
}
