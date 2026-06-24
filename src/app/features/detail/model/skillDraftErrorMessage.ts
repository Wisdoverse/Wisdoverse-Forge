const RAW_SERVICE_DETAIL =
  /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i
const ERROR_TEXT_KEYS = ['serverError', 'detail', 'error', 'message', 'reason'] as const
const STATUS_CODE_KEYS = ['statusCode', 'status', 'code'] as const

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function payloadText(error: unknown, depth = 0): string {
  if (depth > 2) return ''
  const text = errorText(error)
  if (text.trim()) return text
  if (!error || typeof error !== 'object') return ''

  for (const key of ERROR_TEXT_KEYS) {
    const nested = payloadText((error as Record<string, unknown>)[key], depth + 1)
    if (nested.trim()) return nested
  }

  return ''
}

function structuredErrorText(error: unknown): string {
  return payloadText(error)
}

function payloadStatusCode(error: unknown, depth = 0): number | null {
  if (depth > 2 || !error || typeof error !== 'object') return null

  for (const key of STATUS_CODE_KEYS) {
    const value = (error as Record<string, unknown>)[key]
    if (typeof value === 'number' && Number.isFinite(value)) return value
    if (typeof value === 'string' && /^\d{3}$/.test(value.trim())) {
      return Number.parseInt(value, 10)
    }
  }

  for (const key of ERROR_TEXT_KEYS) {
    const nested = payloadStatusCode((error as Record<string, unknown>)[key], depth + 1)
    if (nested != null) return nested
  }

  return null
}

function statusCode(error: unknown): number | null {
  const structuredCode = payloadStatusCode(error)
  if (structuredCode != null) return structuredCode

  const match = structuredErrorText(error).match(
    /\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i
  )
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(error: unknown): boolean {
  const text = structuredErrorText(error).toLowerCase()
  return (
    error instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function skillDraftErrorMessage(error: unknown): string {
  const failure = 'Saved instruction was not saved.'
  const text = structuredErrorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('unauthorized') || text.includes('sign in again')) {
    return `Sign in again, reopen this task, and save the instruction again. ${failure}`
  }
  if (
    code === 403 ||
    text.includes('forbidden') ||
    text.includes('permission') ||
    text.includes('role required') ||
    text.includes('let you create saved instructions') ||
    text.includes('cannot create workspace instructions')
  ) {
    return `Ask an owner or admin to let you create saved instructions, then save again. ${failure}`
  }
  if (code === 404) {
    return `Open this task again, then save the instruction again. ${failure} Saved instruction access may have changed.`
  }
  if (
    code === 409 ||
    text.includes('already exists') ||
    text.includes('already exist') ||
    text.includes('duplicate')
  ) {
    return `Rename it, then save again. A saved instruction with this name may already exist. ${failure}`
  }
  if (RAW_SERVICE_DETAIL.test(text)) {
    return 'Wait a few minutes, then save again. Forge could not save this instruction right now. If it still fails, ask an owner or admin to check Saved instructions access.'
  }
  if (code === 422 || text.includes('validation')) {
    return `Check the name, matching words, and reusable instructions, then save again. ${failure}`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `Wait a minute, then save again. Too many instruction changes are happening right now. ${failure}`
  }
  if (code != null && code >= 500) {
    return 'Wait a few minutes, then save again. Forge could not save this instruction right now. If it still fails, ask an owner or admin to check Saved instructions access.'
  }
  if (isNetworkError(error)) {
    return 'Check your connection, then save again. Forge could not connect while saving this instruction.'
  }

  return `Check the draft, then save again. ${failure} If it still fails, ask an owner or admin to check Saved instructions access.`
}
