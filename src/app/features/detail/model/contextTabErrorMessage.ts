function rawErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function statusCode(err: unknown): number | null {
  const match = rawErrorMessage(err).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(err: unknown): boolean {
  const text = rawErrorMessage(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function contextTabErrorMessage(err: unknown): string {
  const base = 'Task context could not be loaded.'
  const code = statusCode(err)
  const text = rawErrorMessage(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return `${base} Sign in again, then reopen this task.`
  }
  if (code === 403 || text.includes('forbidden')) {
    return `${base} Ask an owner or admin to give you access to this task's context.`
  }
  if (code === 404) {
    return `${base} Refresh the page; this task or its context may have changed.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many context requests are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The platform is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(err)) {
    return `${base} Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this task.`
}
