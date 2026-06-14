type PlatformKeyAction = 'load' | 'create' | 'remove'

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

function baseMessage(action: PlatformKeyAction): string {
  if (action === 'create') return 'Outside tool access key could not be created.'
  if (action === 'remove') return 'Outside tool access key could not be removed.'
  return 'Refresh Settings to load outside tool access keys.'
}

export function platformKeyErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const base = baseMessage(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `${base} Your sign-in expired. Sign in again, then open Settings and try outside tool access again.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return `${base} Ask an owner or admin to let you create or remove outside tool access keys.`
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return `${base} An outside tool access key with this name already exists. Refresh the list, then choose a different name or remove the old key first.`
  }
  if (
    code === 422 ||
    lower.includes('name is required') ||
    lower.includes('name required') ||
    lower.includes('invalid name')
  ) {
    return `${base} Enter the tool or job name, then try again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} Forge is receiving too many outside tool access requests right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    if (action === 'load') {
      return `${base} If it still fails, ask an owner or admin to check outside tool access settings.`
    }
    return `${base} Refresh Settings, then try again. If it still fails, ask an owner or admin to check outside tool access settings.`
  }
  if (isNetworkError(error)) {
    return `${base} Forge could not connect while opening outside tool access settings. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check outside tool access settings.`
}
