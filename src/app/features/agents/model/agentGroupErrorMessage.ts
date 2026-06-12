function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string') return err
  if (!err || typeof err !== 'object') return ''

  const value = err as {
    detail?: unknown
    error?: unknown
    message?: unknown
    reason?: unknown
  }

  for (const candidate of [value.detail, value.error, value.message, value.reason]) {
    if (typeof candidate === 'string' && candidate.trim()) return candidate.trim()
  }

  return ''
}

function statusCode(err: unknown): number | null {
  if (err && typeof err === 'object') {
    const value = err as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      const code = numericStatus(candidate)
      if (code) return code
    }
  }

  const match = errorText(err).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function numericStatus(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && /^\d+$/.test(value)) return Number(value)
  return null
}

function isNetworkError(err: unknown): boolean {
  const text = errorText(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function agentGroupErrorMessage(err: unknown): string {
  const base = 'Task queue was not created.'
  const code = statusCode(err)
  const text = errorText(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return `${base} Sign in again, then choose the project and try again.`
  }
  if (code === 403 || text.includes('forbidden')) {
    return `${base} Ask an owner or admin to let you create and manage task queues in this project.`
  }
  if (code === 404) {
    return `${base} Refresh this page. The selected project may have changed or been removed.`
  }
  if (code === 409) {
    return `${base} A queue with this name may already exist. Use a different name, then try again.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many queue changes are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} Forge could not create the task queue right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check task queue setup.`
  }
  if (isNetworkError(err)) {
    return `${base} Forge could not connect while creating the task queue. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this project's task queue setup.`
}
