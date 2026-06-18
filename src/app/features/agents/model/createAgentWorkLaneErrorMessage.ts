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
    text.includes('load failed')
  )
}

export function createAgentWorkLaneErrorMessage(error: unknown): string {
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('unauthorized') || text.includes('sign in again')) {
    return 'Sign in again, reopen Create Agent, and set up where tasks wait again. The waiting place was not created.'
  }
  if (code === 403 || text.includes('forbidden') || text.includes('permission')) {
    return 'Ask an owner or admin to let you set up where tasks wait in this project. The waiting place was not created.'
  }
  if (code === 404) {
    return 'Refresh this page, then choose the project again. The waiting place was not created because the selected project may have changed or been removed.'
  }
  if (
    code === 409 ||
    text.includes('already exists') ||
    text.includes('already exist') ||
    text.includes('duplicate')
  ) {
    return 'Refresh the project, then choose the existing waiting place. A starter waiting place may already exist.'
  }
  if (code === 422 || text.includes('validation')) {
    return 'Choose a project first, then set up where tasks wait again. The waiting place was not created.'
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return 'Wait a minute, then set up where tasks wait again. Too many changes are happening right now.'
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then set up where tasks wait again. Forge could not create the waiting place right now. If it still fails, ask an owner or admin to check task routing setup.'
  }
  if (isNetworkError(error)) {
    return 'Check your connection, then set up where tasks wait again. Forge could not connect while creating the waiting place.'
  }

  return "Set up where tasks wait again. If it still fails, ask an owner or admin to check this project's task routing setup. The waiting place was not created."
}
