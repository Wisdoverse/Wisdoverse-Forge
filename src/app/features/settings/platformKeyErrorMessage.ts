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
  if (action === 'create') return 'Platform access key could not be created.'
  if (action === 'remove') return 'Platform access key could not be removed.'
  return 'Platform access keys could not be loaded.'
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
    return `${base} Ask an owner or admin to let you create or remove platform access keys.`
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return `${base} A platform access key with this name already exists. Refresh the list, then choose a different name or remove the old key first.`
  }
  if (
    code === 422 ||
    lower.includes('name is required') ||
    lower.includes('name required') ||
    lower.includes('invalid name')
  ) {
    return `${base} Enter the app, script, or workflow name, then try again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} The service is busy. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The platform access key service is temporarily unavailable. Try again. If it still fails, ask an owner to check platform access key settings.`
  }
  if (isNetworkError(error)) {
    return `${base} The app could not reach the service. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner to check platform access key settings.`
}
