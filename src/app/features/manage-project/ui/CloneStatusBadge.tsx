import { useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { projectApi, type CloneStatus, type CloneSummary } from '@app/entities/navigation/project'

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
  dot: string
  pulse?: boolean
}

const VISUALS: Record<Exclude<CloneStatus, 'none'>, Visual> = {
  queued: {
    label: 'Code copy waiting',
    dot: 'bg-apple-orange',
  },
  cloning: {
    label: 'Copying code…',
    dot: 'bg-apple-blue',
    pulse: true,
  },
  ready: {
    label: 'Code copied',
    dot: 'bg-apple-green',
  },
  failed: {
    label: 'Code copy needs help',
    dot: 'bg-apple-red',
  },
}

const CLONE_RETRY_DEFAULT_ERROR =
  'Open Settings, then Projects, check the code link and saved code access, then choose Copy code again for this project in the list. Forge could not copy code into the project.'

function cloneFailureMessage(clone: CloneSummary | undefined): string {
  switch (clone?.errorClass) {
    case 'auth':
      return 'Open Settings and Code access, check saved access for this code project, then choose Copy code again. The code website rejected Forge access.'
    case 'not_found':
      return 'Open Settings, then Projects, check this project code link, then choose Copy code again. Forge could not find this code project.'
    case 'network':
      return 'Check your connection and this project code link, then choose Copy code again. Forge could not reach this code project.'
    case 'timeout':
      return 'Wait a few minutes, then choose Copy code again. The code website took too long to respond.'
    case 'too_large':
      return 'Ask an owner or admin to check project storage, then choose Copy code again if the project can be smaller. This code project is too large to copy right now.'
    case 'internal':
      return 'Wait a few minutes, then choose Copy code again. Forge could not finish copying code.'
    default:
      return 'Open Settings, then Projects, check the code link and saved code access, then choose Copy code again. Forge could not finish copying code.'
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

  const detail = error instanceof Error ? error.message : error
  if (typeof detail === 'string' && detail.toLowerCase().includes('role required')) return 403
  return parseStatusCode(detail)
}

function cloneRetryErrorMessage(error: unknown): string {
  const code = statusCodeFromError(error)
  if (code === 401) {
    return 'Sign in again, then open Settings, then Projects and choose Copy code again for this project in the list.'
  }
  if (code === 403) {
    return 'Ask an owner or admin to let you copy code into this project, then open Settings, then Projects and choose Copy code again. You do not have permission right now.'
  }
  if (code === 404) {
    return 'Open Settings, then Projects, find this project in the list, then choose Copy code again. This project could not be found.'
  }
  if (code === 409) {
    return 'Wait a moment, then check this project in the list again. Forge is already copying code for this project.'
  }
  if (code === 422) {
    return 'Open Settings, then Projects, check the code link and saved code access, then choose Copy code again.'
  }
  if (code === 429) {
    return 'Wait a minute, then choose Copy code again for this project in the list. Too many copy retries are happening right now.'
  }
  if (code && code >= 500) {
    return 'Wait a few minutes, then choose Copy code again for this project in the list. Forge could not copy code right now. If it still fails, ask an owner or admin to check project code access.'
  }

  const message = error instanceof Error ? error.message : typeof error === 'string' ? error : ''
  if (/failed to fetch|network|load failed/i.test(message)) {
    return 'Check your connection, then choose Copy code again for this project in the list.'
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
 * status as a compact dot and label (queued / cloning / ready / failed),
 * and — in the `detail` variant — the resolved branch + short HEAD on success,
 * beginner recovery guidance plus a copy-again action on failure. The action
 * is enabled only for `failed`; permission is enforced server-side (a 403
 * surfaces as an inline message rather than being pre-guarded in the client).
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

  const { label, dot, pulse } = visual
  const isFailed = status === 'failed'
  const failureMessage = isFailed ? cloneFailureMessage(clone) : null
  const progressMessage =
    status === 'queued'
      ? 'Forge will start copying code soon. You can keep this page open; the status updates automatically.'
      : status === 'cloning'
        ? 'Forge is copying code now. You can keep working while it finishes.'
        : status === 'ready'
          ? 'Agents can use this copied code for tasks in this project.'
          : null

  const statusLabel = (
    <span
      data-testid={`clone-status-${projectId}`}
      data-clone-status={status}
      className="inline-flex h-6 shrink-0 items-center gap-1.5 text-ui-body font-medium text-secondary-light dark:text-secondary-dark"
    >
      <span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', dot, pulse && 'animate-pulse')} />
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
        className={cn('h-2 w-2 shrink-0 rounded-full', dot, pulse && 'animate-pulse', className)}
      />
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
        {statusLabel}
        {isFailed && (
          <button
            type="button"
            onClick={() => void handleRetry()}
            disabled={retrying}
            data-testid={`clone-retry-${projectId}`}
            className={cn(
              uiStyles.secondaryButton,
              'h-6 border-apple-red/30 px-2 text-ui-caption text-apple-red hover:bg-apple-red/10 dark:border-apple-red/30 dark:text-apple-red'
            )}
          >
            <RefreshCw
              size={12}
              strokeWidth={2.25}
              aria-hidden="true"
              className={cn(retrying && 'animate-spin')}
            />
            {retrying ? 'Copying code…' : 'Copy code again'}
          </button>
        )}
      </div>

      {failureMessage && (
        <p className="text-ui-caption text-apple-red" role="status">
          {failureMessage}
        </p>
      )}
      {progressMessage && (
        <p className="text-ui-caption text-secondary-light dark:text-secondary-dark" role="status">
          {progressMessage}
        </p>
      )}
      {retryError && (
        <p className="text-ui-caption text-apple-red" role="alert" aria-live="polite">
          {retryError}
        </p>
      )}
    </div>
  )
}
