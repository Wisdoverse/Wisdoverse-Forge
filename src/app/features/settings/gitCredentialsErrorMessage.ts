type GitCredentialAction = 'load' | 'save' | 'remove'

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (!error || typeof error !== 'object') return ''

  const value = error as {
    detail?: unknown
    error?: unknown
    message?: unknown
    reason?: unknown
  }

  for (const candidate of [value.detail, value.error, value.message, value.reason]) {
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

function actionFromText(text: string): GitCredentialAction {
  if (/\b(save|saving|saved|create|created|update|updated)\b/i.test(text)) return 'save'
  if (/\b(delete|deleted|remove|removed|removing)\b/i.test(text)) return 'remove'
  return 'load'
}

function baseMessage(action: GitCredentialAction): string {
  if (action === 'save') return 'Repository access could not be saved.'
  if (action === 'remove') return 'Repository access could not be removed.'
  return 'Repository access could not be loaded.'
}

export function gitCredentialsErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const base = baseMessage(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `${base} Your sign-in expired. Sign in again, then open Settings and try repository access again.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return `${base} Ask an owner or admin to let you manage repository access.`
  }
  if (
    lower.includes('invalid token') ||
    lower.includes('bad credentials') ||
    lower.includes('expired token') ||
    lower.includes('token expired')
  ) {
    return `${base} Paste a new GitHub or GitLab access key, then save again.`
  }
  if (code === 409 || lower.includes('already exists')) {
    return `${base} Repository access for this GitHub or GitLab choice already exists. Remove the old entry first or choose the other site.`
  }
  if (code === 422 || lower.includes('invalid host') || lower.includes('invalid provider')) {
    return `${base} Check the selected site, access key, and GitHub or GitLab address, then try again.`
  }
  if (
    lower.includes('not configured') ||
    lower.includes('provider is not configured') ||
    lower.includes('provider not configured')
  ) {
    return `${base} Ask an owner or admin to check repository access settings, then try again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} Forge is receiving too many repository access requests right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} Refresh Settings, then try again. If it still fails, ask an owner or admin to check repository access settings.`
  }
  if (isNetworkError(error)) {
    return `${base} Forge could not connect while opening repository access. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check repository access settings.`
}
