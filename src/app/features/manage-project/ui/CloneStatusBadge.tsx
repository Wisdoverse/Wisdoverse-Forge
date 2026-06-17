import { useState } from 'react'
import { CheckCircle2, CircleDashed, GitBranch, Loader2, RefreshCw, XCircle } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { projectApi, type CloneStatus, type CloneSummary } from '@app/entities/project'

interface CloneStatusBadgeProps {
  projectId: string
  status: CloneStatus | undefined
  clone?: CloneSummary
  /**
   * `compact` is the sidebar tree indicator (an icon-only status dot with a
   * tooltip — no label text or retry, to fit the narrow tree). `detail` is the
   * settings/detail surface (head sha/branch on success, beginner recovery
   * guidance + a Retry action on failure).
   */
  variant?: 'compact' | 'detail'
  /** Called after a successful retry with the new attempt summary. */
  onRetried?: (summary: CloneSummary) => void
  className?: string
}

type Visual = {
  label: string
  /** `bg-<tint> text-<tone>` classes for the pill body. */
  tint: string
  Icon: typeof CheckCircle2
  spin?: boolean
}

const VISUALS: Record<Exclude<CloneStatus, 'none'>, Visual> = {
  queued: {
    label: 'Code import queued',
    tint: 'bg-apple-orange/10 text-apple-orange',
    Icon: CircleDashed,
  },
  cloning: {
    label: 'Copying code…',
    tint: 'bg-apple-blue/10 text-apple-blue',
    Icon: Loader2,
    spin: true,
  },
  ready: {
    label: 'Code ready',
    tint: 'bg-apple-green/10 text-apple-green',
    Icon: CheckCircle2,
  },
  failed: {
    label: 'Code import failed',
    tint: 'bg-apple-red/10 text-apple-red',
    Icon: XCircle,
  },
}

const CLONE_RETRY_DEFAULT_ERROR =
  'Check the code link and saved code access, then try copying code again. Forge could not copy code into the project.'

function cloneFailureMessage(clone: CloneSummary | undefined): string {
  switch (clone?.errorClass) {
    case 'auth':
      return 'Check saved code access for this repository, then try copying code again. The repository rejected Forge access.'
    case 'not_found':
      return 'Check the code link, then try copying code again. Forge could not find this repository.'
    case 'network':
      return 'Check your connection and repository host, then try copying code again. Forge could not reach the repository.'
    case 'timeout':
      return 'Wait a few minutes, then try copying code again. The repository took too long to respond.'
    case 'too_large':
      return 'Ask an owner or admin to check project storage before trying again. This repository is too large to copy right now.'
    case 'internal':
      return 'Wait a few minutes, then try copying code again. Forge could not finish the code import.'
    default:
      return 'Check the code link and saved code access, then try copying code again. Forge could not finish the code import.'
  }
}

function parseStatusCode(value: unknown): number | null {
  if (typeof value === 'number' && Number.isInteger(value) && value >= 100 && value <= 599) {
    return value
  }

  if (typeof value !== 'string') return null

  const trimmed = value.trim()
  if (/^\d{3}$/.test(trimmed)) {
    const parsed = Number.parseInt(trimmed, 10)
    return parsed >= 100 && parsed <= 599 ? parsed : null
  }

  const match = trimmed.match(/\b(?:api|http|status|code)\s*[:#]?\s*(\d{3})\b/i)
  if (!match) return null

  const parsed = Number.parseInt(match[1], 10)
  return parsed >= 100 && parsed <= 599 ? parsed : null
}

function statusCodeFromError(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const fields = error as Record<string, unknown>
    for (const key of ['statusCode', 'status', 'code'] as const) {
      const parsed = parseStatusCode(fields[key])
      if (parsed) return parsed
    }
  }

  return parseStatusCode(error instanceof Error ? error.message : error)
}

function cloneRetryErrorMessage(error: unknown): string {
  const code = statusCodeFromError(error)
  if (code === 401) return 'Sign in again, then try copying code again from the project row.'
  if (code === 403) {
    return 'Ask an owner or admin to let you copy code into this project, then try again. You do not have permission right now.'
  }
  if (code === 404) {
    return 'Refresh Projects, then try copying code again from the current project row. This project could not be found.'
  }
  if (code === 409) {
    return 'Wait a moment, then check the status again. Forge is already copying code for this project.'
  }
  if (code === 422) {
    return 'Check the code link and saved code access, then try copying code again.'
  }
  if (code === 429) {
    return 'Wait a minute, then try copying code again. Too many code import retries are happening right now.'
  }
  if (code && code >= 500) {
    return 'Wait a few minutes, then try copying code again. Forge could not copy code right now. If it still fails, ask an owner or admin to check project code setup.'
  }

  const message = error instanceof Error ? error.message : typeof error === 'string' ? error : ''
  if (/failed to fetch|network|load failed/i.test(message)) {
    return 'Check your connection, then try copying code again.'
  }

  return CLONE_RETRY_DEFAULT_ERROR
}

/** Show the badge only for projects with an actual clone lifecycle. */
function visualFor(status: CloneStatus | undefined): Visual | null {
  if (!status || status === 'none') return null
  return VISUALS[status]
}

/**
 * Clone lifecycle badge for a project's optional git repository. Renders the
 * status as an Apple-style pill (queued / cloning with spinner / ready / failed),
 * and — in the `detail` variant — the resolved branch + short HEAD on success,
 * beginner recovery guidance plus a Retry action on failure. Retry is enabled
 * only for `failed`; permission is enforced server-side (a 403 surfaces as an
 * inline message rather than being pre-guarded in the client).
 */
export function CloneStatusBadge({
  projectId,
  status,
  clone,
  variant = 'compact',
  onRetried,
  className,
}: CloneStatusBadgeProps) {
  const [retrying, setRetrying] = useState(false)
  const [retryError, setRetryError] = useState<string | null>(null)

  const visual = visualFor(status)
  if (!visual) return null

  const { label, tint, Icon, spin } = visual
  const isFailed = status === 'failed'
  const isReady = status === 'ready'
  const shortSha = clone?.headSha ? clone.headSha.slice(0, 7) : null
  const branch = clone?.resolvedBranch ?? null
  const failureMessage = isFailed ? cloneFailureMessage(clone) : null

  const pill = (
    <span
      data-testid={`clone-status-${projectId}`}
      data-clone-status={status}
      className={cn(
        'inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full px-2 text-ui-caption font-medium',
        tint
      )}
    >
      <Icon
        size={12}
        strokeWidth={2.25}
        aria-hidden="true"
        className={cn(spin && 'animate-spin')}
      />
      <span className="truncate">{label}</span>
    </span>
  )

  if (variant === 'compact') {
    // Icon-only dot for the narrow sidebar tree; the status reads out via title +
    // aria-label so it stays accessible without spending horizontal space.
    return (
      <span
        data-testid={`clone-status-${projectId}`}
        data-clone-status={status}
        title={label}
        aria-label={label}
        role="status"
        className={cn(
          'inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full',
          tint,
          className
        )}
      >
        <Icon
          size={11}
          strokeWidth={2.5}
          aria-hidden="true"
          className={cn(spin && 'animate-spin')}
        />
      </span>
    )
  }

  async function handleRetry() {
    setRetrying(true)
    setRetryError(null)
    try {
      const summary = await projectApi.retryClone(projectId)
      onRetried?.(summary)
    } catch (err) {
      setRetryError(cloneRetryErrorMessage(err))
    } finally {
      setRetrying(false)
    }
  }

  return (
    <div className={cn('flex flex-col gap-1.5', className)}>
      <div className="flex flex-wrap items-center gap-2">
        {pill}
        {isReady && (branch || shortSha) && (
          <span className="inline-flex items-center gap-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            <GitBranch size={12} strokeWidth={2} aria-hidden="true" />
            {branch ? <span className="truncate">{branch}</span> : null}
            {shortSha ? (
              <span className="font-mono text-[11px] text-secondary-light dark:text-secondary-dark">
                {shortSha}
              </span>
            ) : null}
          </span>
        )}
        {isFailed && (
          <button
            type="button"
            onClick={() => void handleRetry()}
            disabled={retrying}
            data-testid={`clone-retry-${projectId}`}
            className={cn(
              'inline-flex h-6 items-center gap-1 rounded-full border border-apple-red/30 px-2 text-ui-caption font-semibold text-apple-red transition-colors',
              'hover:bg-apple-red/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30',
              'disabled:cursor-not-allowed disabled:opacity-60'
            )}
          >
            <RefreshCw
              size={12}
              strokeWidth={2.25}
              aria-hidden="true"
              className={cn(retrying && 'animate-spin')}
            />
            {retrying ? 'Trying…' : 'Try again'}
          </button>
        )}
      </div>

      {failureMessage && (
        <p className="text-ui-caption text-apple-red" role="status">
          {failureMessage}
        </p>
      )}
      {retryError && (
        <p className="text-ui-caption text-apple-red" role="alert">
          {retryError}
        </p>
      )}
    </div>
  )
}
