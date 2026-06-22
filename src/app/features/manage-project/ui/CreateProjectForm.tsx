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
  'Choose whether to start without code or copy code now.',
  'Create the project. If code is being copied, watch this project in the list for status.',
]

type CodeSetupMode = 'later' | 'copy'

/**
 * Validate the optional code link in the SUBMIT HANDLER (not via
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
    return 'Paste the https:// code link from your browser, or leave this blank and set up SSH code access in Settings first.'
  }

  let parsed: URL
  try {
    parsed = new URL(value)
  } catch {
    return 'Paste a full GitHub or GitLab code link, for example https://github.com/team/project.git, or leave this blank.'
  }
  if (parsed.protocol !== 'https:') {
    return 'Paste the https:// code link from your browser, or leave this blank and set up SSH code access in Settings first.'
  }
  // No credentials embedded in the URL (user[:pass]@host) — the server rejects
  // these so a token never lands in a stored URL. `URL` also flags a bare `@`.
  if (parsed.username || parsed.password || value.includes('@')) {
    return 'Remove account details from the code link. Save code access in Settings instead, then create this project again.'
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
    return 'Open Settings, then Projects again, choose the team, then create this project.'
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
    return 'Paste an https:// code link without account details, or leave the code link blank and add code access in Settings.'
  }
  if (
    lower.includes('credential') ||
    lower.includes('token') ||
    lower.includes('password') ||
    lower.includes('username')
  ) {
    return 'Remove account details from the code link. Save code access in Settings instead, then create this project again.'
  }
  if (code === 422 || lower.includes('validation') || lower.includes('invalid')) {
    return 'Check the project name, team, and code link. You can leave the code link blank, then create this project again.'
  }
  if (code === 429 || lower.includes('rate limit') || lower.includes('too many')) {
    return 'Wait a minute, then create this project again. Too many project changes are happening right now.'
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then create this project again. Forge could not create the project right now. If it still fails, ask an owner or admin to check Projects in Settings.'
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
  const [codeSetupMode, setCodeSetupMode] = useState<CodeSetupMode>('later')
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
  const codeSetupLaterId = 'create-project-code-setup-later'
  const codeSetupCopyId = 'create-project-code-setup-copy'
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
  const workspaceFolderName = trimmedName ? slugifyName(name) : null
  const workspacePath = workspaceFolderName ? `/workspace/${workspaceFolderName}` : null
  const copyCodeNow = codeSetupMode === 'copy'
  const trimmedRepositoryUrl = copyCodeNow ? repositoryUrl.trim() : ''
  const codeLinkStatus = copyCodeNow
    ? trimmedRepositoryUrl
      ? 'Code copy requested. After creation, watch this project in the list for Code copy waiting, Copying code, or Code copied. If it needs help, choose Copy code again.'
      : 'Copy code now selected. Paste an https:// code link below, or choose Create without code.'
    : 'No code link added. Create the project now, then add code access later if agents need files.'
  const readyStatus =
    copyCodeNow && trimmedRepositoryUrl
      ? 'Ready to create project and copy code'
      : 'Ready to create project'

  useEffect(() => {
    if (!teamId && teams[0]) {
      setTeamId(teams[0].id)
    }
  }, [teamId, teams])

  function focusTop() {
    formRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  function chooseCodeSetupMode(mode: CodeSetupMode) {
    setCodeSetupMode(mode)
    setBannerError(null)
    if (mode === 'later') {
      setRepositoryUrl('')
    }
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSubmitAttempted(true)
    setBannerError(null)

    if (!isReady) {
      document.getElementById(missingTeam ? teamSelectId : nameInputId)?.focus()
      return
    }

    // Validate the optional code link in the handler so an invalid value shows a
    // banner + blocks submit (no silent dead-click), instead of relying on
    // formState.errors the modal never renders.
    const repoError = copyCodeNow ? validateRepositoryUrl(repositoryUrl) : null
    if (repoError) {
      setBannerError(repoError)
      focusTop()
      document.getElementById(repoInputId)?.focus()
      return
    }

    const trimmedRepo = copyCodeNow ? repositoryUrl.trim() : ''
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
        <div id={bannerId} role="alert" aria-live="polite" className={uiStyles.error}>
          {bannerError}
        </div>
      )}

      <div className="mb-4 border-l-2 border-apple-blue/40 pl-3">
        <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
          Project creation steps
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          Use projects to keep one work area&apos;s tasks, files, and saved work together.
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
            Pick the name users will look for when sending tasks.
          </p>
          {trimmedName && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Project menu link preview: {slugifyName(name)}. Forge creates it automatically from
              the project name. You do not need to type it.
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
        <fieldset>
          <legend className={uiStyles.label}>Choose code setup</legend>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <label
              htmlFor={codeSetupLaterId}
              className={cn(
                'flex min-h-24 cursor-pointer gap-2 rounded-lg border px-3 py-2 transition-colors',
                !copyCodeNow
                  ? 'border-apple-blue/45 bg-apple-blue/5'
                  : 'border-black/[0.08] bg-white hover:bg-black/[0.02] dark:border-white/[0.1] dark:bg-white/[0.04] dark:hover:bg-white/[0.07]'
              )}
            >
              <input
                id={codeSetupLaterId}
                type="radio"
                name="create-project-code-setup"
                checked={!copyCodeNow}
                onChange={() => chooseCodeSetupMode('later')}
                className="mt-1 h-4 w-4 shrink-0 accent-apple-blue"
              />
              <span>
                <span className="block text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                  Create this project without code
                </span>
                <span className="mt-1 block text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Use this when you want a place for tasks now. You can add code access later in
                  Settings.
                </span>
              </span>
            </label>
            <label
              htmlFor={codeSetupCopyId}
              className={cn(
                'flex min-h-24 cursor-pointer gap-2 rounded-lg border px-3 py-2 transition-colors',
                copyCodeNow
                  ? 'border-apple-blue/45 bg-apple-blue/5'
                  : 'border-black/[0.08] bg-white hover:bg-black/[0.02] dark:border-white/[0.1] dark:bg-white/[0.04] dark:hover:bg-white/[0.07]'
              )}
            >
              <input
                id={codeSetupCopyId}
                type="radio"
                name="create-project-code-setup"
                checked={copyCodeNow}
                onChange={() => chooseCodeSetupMode('copy')}
                className="mt-1 h-4 w-4 shrink-0 accent-apple-blue"
              />
              <span>
                <span className="block text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
                  Copy code now from GitHub or GitLab
                </span>
                <span className="mt-1 block text-ui-caption text-secondary-light dark:text-secondary-dark">
                  Use this when agents need project files right away and you have an https:// code
                  link.
                </span>
              </span>
            </label>
          </div>
        </fieldset>

        {copyCodeNow && (
          <div className="mt-3">
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
              placeholder="https://github.com/team/project.git"
              aria-describedby="project-repo-help project-repo-status"
              className={inputClass}
            />
            <div
              id="project-repo-help"
              className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              <p className="font-medium text-foreground-light dark:text-foreground-dark">
                Copy code now
              </p>
              <ol className="mt-1 list-decimal space-y-1 pl-4">
                <li>Open the project on GitHub or GitLab and choose Code, then HTTPS.</li>
                <li>Paste the https:// code link below.</li>
                <li>Create the project. Watch this project in the list for copy status.</li>
              </ol>
              <p className="mt-1">
                If you only see an SSH link, choose Create without code, then set up SSH code access
                in Settings first. Never paste passwords or access keys here.
              </p>
            </div>
          </div>
        )}
        <p
          id="project-repo-status"
          data-testid="create-project-code-link-status"
          className="mt-1 rounded-md bg-black/[0.025] px-2 py-1 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark"
        >
          {codeLinkStatus}
        </p>
        {workspaceFolderName && workspacePath && (
          <div className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            <p>
              Agents will open this project in a folder named{' '}
              <span className="font-medium text-foreground-light dark:text-foreground-dark">
                {workspaceFolderName}
              </span>
              . You do not need to type this.
            </p>
            <details className="mt-1">
              <summary className="cursor-pointer text-apple-blue hover:underline">
                Show folder details for support
              </summary>
              <p className="mt-1">
                Use this only if an owner, admin, or support message asks for the project folder.
              </p>
              <span className="font-mono text-[11px] text-foreground-light dark:text-foreground-dark">
                Project folder for support: {workspacePath}
              </span>
            </details>
          </div>
        )}
      </div>

      {visibleError && errorField === 'name' && (
        <p id={errorId} role="alert" aria-live="polite" className="text-ui-caption text-apple-red">
          {visibleError}
        </p>
      )}
      {visibleError && errorField === 'team' && (
        <p role="alert" aria-live="polite" className="text-ui-caption text-apple-red">
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
            ? readyStatus
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
