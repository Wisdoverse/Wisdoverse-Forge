function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
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

function statusCode(error: unknown): number | null {
  if (error && typeof error === 'object') {
    const value = error as { status?: unknown; statusCode?: unknown; code?: unknown }
    for (const candidate of [value.status, value.statusCode, value.code]) {
      if (typeof candidate === 'number' && Number.isFinite(candidate)) return candidate
      if (typeof candidate === 'string' && /^\d+$/.test(candidate)) return Number(candidate)
    }
  }

  const match = errorText(error).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(error: unknown): boolean {
  const text = errorText(error).toLowerCase()
  return (
    error instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('browser could not reach') ||
    text.includes('load failed')
  )
}

/**
 * Maps a control-plane fetch failure to operator-facing copy with a recovery
 * step. Mirrors the shape of `systemHealthErrorMessage`; accepts the thrown
 * error or a plain error string from the admin store.
 */
export function controlPlaneErrorMessage(error: unknown): string {
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return 'Sign in again, then open Admin and choose Agent coordination before choosing Check again. Forge could not load agent coordination status because your sign-in expired.'
  }
  if (
    code === 403 ||
    text.includes('permission') ||
    text.includes('forbidden') ||
    text.includes('role required')
  ) {
    return 'Ask an owner or admin to give you Admin access, then open Admin and choose Agent coordination before choosing Check again. You do not have access to agent coordination status.'
  }
  if (code === 404 || text.includes('endpoint is not available')) {
    return 'Open Admin and choose Agent coordination, then choose Check again. Agent coordination status is not available from this Admin view. If it still fails, ask an owner or admin to check Agent coordination in Admin.'
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return 'Wait a minute, then open Admin, choose Agent coordination, and choose Check again. Forge is receiving too many requests right now.'
  }
  if (code === 503 || (code != null && code >= 500)) {
    return 'Open Admin and choose Agent coordination, then choose Check again. Forge could not load agent coordination status. If it still fails, ask an owner or admin to check Agent coordination in Admin.'
  }
  if (isNetworkError(error)) {
    return 'Check your connection, then open Admin and choose Agent coordination before choosing Check again. Forge could not connect while loading agent coordination status.'
  }

  return 'Open Admin and choose Agent coordination, then choose Check again. Forge could not load agent coordination status. If it still fails, ask an owner or admin to check Agent coordination in Admin.'
}
