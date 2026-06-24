const RAW_SERVICE_DETAIL =
  /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i

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
    const text = payloadText(candidate)
    if (text) return text
  }

  return ''
}

function payloadText(value: unknown): string | null {
  if (typeof value === 'string' && value.trim()) return value.trim()
  if (!value || typeof value !== 'object') return null

  const record = value as Record<string, unknown>
  for (const key of ['serverError', 'message', 'error', 'detail', 'reason']) {
    const text = payloadText(record[key])
    if (text) return text
  }

  return null
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
    return 'Sign in again, open New agent again, and set up the task queue again. The task queue was not created.'
  }
  if (
    code === 403 ||
    text.includes('forbidden') ||
    text.includes('permission') ||
    text.includes('role required')
  ) {
    return 'Ask an owner or admin to let you set up the task queue in this project. The task queue was not created.'
  }
  if (code === 404) {
    return 'Open New agent, choose the project again, then set up the task queue. The task queue was not created because the selected project may have changed or been removed.'
  }
  if (
    code === 409 ||
    text.includes('already exists') ||
    text.includes('already exist') ||
    text.includes('duplicate')
  ) {
    return 'Open the project again, then choose the existing task queue. A starter task queue may already exist.'
  }
  if (RAW_SERVICE_DETAIL.test(text)) {
    return 'Wait a few minutes, then set up the task queue again. Forge could not create the task queue right now. If it still fails, ask an owner or admin to check the task queue in this project.'
  }
  if (code === 422 || text.includes('validation')) {
    return 'Choose a project first, then set up the task queue again. The task queue was not created.'
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return 'Wait a minute, then set up the task queue again. Too many changes are happening right now.'
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then set up the task queue again. Forge could not create the task queue right now. If it still fails, ask an owner or admin to check the task queue in this project.'
  }
  if (isNetworkError(error)) {
    return 'Check your connection, then set up the task queue again. Forge could not connect while creating the task queue.'
  }

  return 'Set up the task queue again. If it still fails, ask an owner or admin to check the task queue in this project. The task queue was not created.'
}
