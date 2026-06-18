function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string') return err
  if (!err || typeof err !== 'object') return ''

  const value = err as {
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
  const code = statusCode(err)
  const text = errorText(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return 'Sign in again, choose the project, and try setting up where tasks wait again. The waiting place was not created.'
  }
  if (code === 403 || text.includes('forbidden')) {
    return 'Ask an owner or admin to let you set up where tasks wait in this project. The waiting place was not created.'
  }
  if (code === 404) {
    return 'Refresh this page, then choose the project again. The waiting place was not created because the selected project may have changed or been removed.'
  }
  if (code === 409) {
    return 'Use a different name, then try creating the waiting place again. A waiting place with this name may already exist.'
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return 'Wait a minute, then try creating the waiting place again. Too many waiting-place changes are happening right now.'
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then try setting up where tasks wait again. Forge could not create the waiting place right now. If it still fails, ask an owner or admin to check where tasks wait in this project.'
  }
  if (isNetworkError(err)) {
    return 'Check your connection, then try creating the waiting place again. Forge could not connect while setting up where tasks wait.'
  }

  return 'Try creating the waiting place again. If it still fails, ask an owner or admin to check where tasks wait in this project. The waiting place was not created.'
}
