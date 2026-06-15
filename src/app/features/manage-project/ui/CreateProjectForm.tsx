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
  'Name the project after the product, repo, or work area.',
  'Optionally paste an HTTPS git URL to clone an existing repo into the project.',
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

  let parsed: URL
  try {
    parsed = new URL(value)
  } catch {
    return 'Enter a valid URL, e.g. https://github.com/org/repo.git'
  }
  if (parsed.protocol !== 'https:') {
    return 'Use an https:// URL. SSH and other schemes are not supported here.'
  }
  // No credentials embedded in the URL (user[:pass]@host) — the server rejects
  // these so a token never lands in a stored URL. `URL` also flags a bare `@`.
  if (parsed.username || parsed.password || value.includes('@')) {
    return 'Remove credentials from the URL. Connect a git account in Settings instead.'
  }
  return null
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
      setBannerError(
        err instanceof Error ? err.message : 'Could not create the project. Try again.'
      )
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
          {trimmedName && (
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

      <div className="mb-3">
        <label htmlFor={repoInputId} className={uiStyles.label}>
          Git repository URL
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
          Optional — clone an existing repo into this project. HTTPS only, no credentials in the
          URL.
        </p>
        {workspacePath && (
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Workspace path:{' '}
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
