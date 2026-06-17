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
      const code = numericStatus(candidate)
      if (code) return code
    }
  }

  const match = errorText(error).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function numericStatus(value: unknown): number | null {
  if (typeof value === 'number' && Number.isFinite(value)) return value
  if (typeof value === 'string' && /^\d+$/.test(value)) return Number(value)
  return null
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

export function systemHealthErrorMessage(error: unknown): string {
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return 'Sign in again, then open Admin and choose Check now. Forge could not check app health because your sign-in expired.'
  }
  if (code === 403 || text.includes('permission') || text.includes('forbidden')) {
    return 'Ask an owner or admin to give you Admin access, then choose Check now. Forge could not check app health because you do not have access to app health checks.'
  }
  if (code === 404 || text.includes('endpoint is not available')) {
    return 'Refresh Admin, then choose Check now. App health checks are not available from this Admin view. If it still fails, ask an owner or admin to check setup.'
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return 'Wait a minute, then choose Check now. Forge is receiving too many health checks right now.'
  }
  if (code != null && code >= 500) {
    return 'Refresh Admin, then choose Check now. Forge could not check app health. If it still fails, ask an owner or admin to check app health setup.'
  }
  if (isNetworkError(error)) {
    return 'Check your connection, then choose Check now. Forge could not connect while checking app health.'
  }

  return 'Choose Check now again. Forge could not check app health. If it still fails, ask an owner or admin to check app health setup.'
}
