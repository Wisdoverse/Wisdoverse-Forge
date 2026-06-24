const RAW_SERVICE_DETAIL =
  /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i

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
    return 'Sign in again, choose the project, and set up the task queue again. The task queue was not created.'
  }
  if (code === 403 || text.includes('forbidden') || text.includes('role required')) {
    return 'Ask an owner or admin to let you set up the task queue in this project. The task queue was not created.'
  }
  if (code === 404) {
    return 'Open Agents, choose the project again, then set up the task queue. The task queue was not created because the selected project may have changed or been removed.'
  }
  if (code === 409) {
    return 'Use a different name, then create the task queue again. A task queue with this name may already exist.'
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return 'Wait a minute, then create the task queue again. Too many task queue changes are happening right now.'
  }
  if (RAW_SERVICE_DETAIL.test(text)) {
    return 'Wait a few minutes, then set up the task queue again. Forge could not create the task queue right now. If it still fails, ask an owner or admin to check the task queue in this project.'
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then set up the task queue again. Forge could not create the task queue right now. If it still fails, ask an owner or admin to check the task queue in this project.'
  }
  if (isNetworkError(err)) {
    return 'Check your connection, then create the task queue again. Forge could not connect while setting up the task queue.'
  }

  return 'Create the task queue again. If it still fails, ask an owner or admin to check the task queue in this project. The task queue was not created.'
}
