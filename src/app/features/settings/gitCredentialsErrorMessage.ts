type GitCredentialAction = 'load' | 'save' | 'remove'

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

function actionFromText(text: string): GitCredentialAction {
  const lower = text.toLowerCase()
  if (
    /\b(save|saving|saved|create|created|update|updated)\b/i.test(text) ||
    lower.includes('invalid token') ||
    lower.includes('bad credentials') ||
    lower.includes('expired token') ||
    lower.includes('token expired') ||
    lower.includes('invalid host') ||
    lower.includes('invalid provider')
  ) {
    return 'save'
  }
  if (/\b(delete|deleted|remove|removed|removing)\b/i.test(text)) return 'remove'
  return 'load'
}

function baseMessage(action: GitCredentialAction): string {
  if (action === 'save') return 'Repository access could not be saved.'
  if (action === 'remove') return 'Repository access could not be removed.'
  return 'Refresh Settings to load repository access.'
}

function connectionMessage(action: GitCredentialAction): string {
  if (action === 'load') {
    return 'Check your connection, then refresh Settings to load repository access. Forge could not connect while opening repository access.'
  }
  const verb = action === 'remove' ? 'remove' : 'save'
  return `Check your connection, then ${verb} repository access again. Forge could not connect while opening repository access.`
}

function validationGuidance(lower: string): string {
  if (lower.includes('invalid provider')) {
    return 'Choose GitHub or GitLab, then save repository access again.'
  }
  if (lower.includes('invalid host')) {
    return 'Check the GitHub or GitLab address. Leave it blank for github.com or gitlab.com, then save again.'
  }
  return 'Check the selected site, repository access key, and GitHub or GitLab address, then save again.'
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
    return `${base} Paste a new repository access key from GitHub or GitLab, then save again.`
  }
  if (code === 409 || lower.includes('already exists')) {
    return `${base} Repository access for this GitHub or GitLab choice already exists. Remove the old entry first or choose the other site.`
  }
  if (code === 422 || lower.includes('invalid host') || lower.includes('invalid provider')) {
    return `${base} ${validationGuidance(lower)}`
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
    if (action === 'load') {
      return `${base} If it still fails, ask an owner or admin to check repository access settings.`
    }
    return `${base} Refresh Settings, then try again. If it still fails, ask an owner or admin to check repository access settings.`
  }
  if (isNetworkError(error)) {
    return connectionMessage(action)
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check repository access settings.`
}
