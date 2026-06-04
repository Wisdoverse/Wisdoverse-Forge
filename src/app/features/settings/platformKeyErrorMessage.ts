type PlatformKeyAction = 'load' | 'create' | 'remove'

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function statusCode(error: unknown): number | null {
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
  if (action === 'create') return 'Platform API key could not be created.'
  if (action === 'remove') return 'Platform API key could not be revoked.'
  return 'Platform API keys could not be loaded.'
}

export function platformKeyErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const base = baseMessage(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `${base} Sign in again, then open Settings and try platform keys again.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return `${base} Ask an owner or admin for access to manage platform API keys.`
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return `${base} A key with this name or value already exists. Refresh the list, then choose a different name or revoke the old key first.`
  }
  if (
    code === 422 ||
    lower.includes('name is required') ||
    lower.includes('name required') ||
    lower.includes('invalid name')
  ) {
    return `${base} Enter a short name that says where this key will be used, then try again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} The server is busy. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The platform key service is temporarily unavailable. Ask an owner to check the backend, then try again.`
  }
  if (isNetworkError(error)) {
    return `${base} The browser could not reach the server. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner to check platform key settings.`
}
