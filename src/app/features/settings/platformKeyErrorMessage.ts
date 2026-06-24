type PlatformKeyAction = 'load' | 'create' | 'remove'

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
    text.includes('browser could not reach') ||
    text.includes('load failed')
  )
}

function actionFromText(text: string): PlatformKeyAction {
  const lower = text.toLowerCase()
  if (/\b(create|created|creating|save|saved|saving)\b/i.test(text)) return 'create'
  if (
    lower.includes('required fields for platform api key') ||
    lower.includes('name is required') ||
    lower.includes('name required') ||
    lower.includes('invalid name') ||
    lower.includes('already exists') ||
    lower.includes('duplicate')
  ) {
    return 'create'
  }
  if (/\b(revoke|revoked|revoking|delete|deleted|remove|removed|removing)\b/i.test(text)) {
    return 'remove'
  }
  return 'load'
}

function retryAction(action: PlatformKeyAction): string {
  if (action === 'create') return 'create this tool access key again'
  if (action === 'remove') return 'remove this tool access key again'
  return 'open Settings and Tool access keys again'
}

function connectionMessage(action: PlatformKeyAction): string {
  if (action === 'load') {
    return 'Check your connection, then open Settings and Tool access keys again. Forge could not connect while opening tool access key settings.'
  }
  if (action === 'remove') {
    return 'Check your connection, then remove this tool access key again. The removal did not finish.'
  }
  return 'Check your connection, then create this tool access key again. The creation did not finish.'
}

export function platformKeyErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const retry = retryAction(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `Sign in again, then ${retry}. Your sign-in expired.`
  }
  if (
    code === 403 ||
    lower.includes('permission') ||
    lower.includes('forbidden') ||
    lower.includes('role required')
  ) {
    return 'Ask an owner or admin to let you create or remove tool access keys.'
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return 'Open Settings and Tool access keys again, check the current key, then choose a different name or remove the old key first.'
  }
  if (RAW_SERVICE_DETAIL.test(lower)) {
    if (action === 'load') {
      return 'Open Settings and Tool access keys again. If it still fails, ask an owner or admin to check tool access key settings.'
    }
    return `Open Settings and Tool access keys again, then ${retry}. If it still fails, ask an owner or admin to check tool access key settings.`
  }
  if (
    code === 422 ||
    lower.includes('name is required') ||
    lower.includes('name required') ||
    lower.includes('invalid name')
  ) {
    return `Enter the tool or job name, then ${retry}.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `Wait a minute, then ${retry}. Forge is receiving too many tool access key requests right now.`
  }
  if (code != null && code >= 500) {
    if (action === 'load') {
      return 'Open Settings and Tool access keys again. If it still fails, ask an owner or admin to check tool access key settings.'
    }
    return `Open Settings and Tool access keys again, then ${retry}. If it still fails, ask an owner or admin to check tool access key settings.`
  }
  if (isNetworkError(error)) {
    return connectionMessage(action)
  }

  if (action === 'load') {
    return 'Open Settings and Tool access keys again. If it still fails, ask an owner or admin to check tool access key settings.'
  }

  return `${retry.charAt(0).toUpperCase()}${retry.slice(1)}. If it still fails, ask an owner or admin to check tool access key settings.`
}
