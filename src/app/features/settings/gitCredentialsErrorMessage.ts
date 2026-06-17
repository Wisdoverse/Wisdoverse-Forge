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

function retryAction(action: GitCredentialAction): string {
  if (action === 'save') return 'save code access again'
  if (action === 'remove') return 'remove code access again'
  return 'refresh Settings to load code access'
}

function connectionMessage(action: GitCredentialAction): string {
  if (action === 'load') {
    return 'Check your connection, then refresh Settings to load code access. Forge could not connect while opening code access.'
  }
  if (action === 'remove') {
    return 'Check your connection, then remove code access again. The removal did not finish.'
  }
  return 'Check your connection, then save code access again. The save did not finish.'
}

function validationGuidance(lower: string): string {
  if (lower.includes('invalid provider')) {
    return 'Choose GitHub or GitLab, then save code access again.'
  }
  if (lower.includes('invalid host')) {
    return 'Check the GitHub or GitLab address. Leave it blank for github.com or gitlab.com, then save again.'
  }
  return 'Check the selected site, code access key, and GitHub or GitLab address, then save again.'
}

export function gitCredentialsErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const retry = retryAction(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `Sign in again, then ${retry}. Your sign-in expired.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return 'Ask an owner or admin to let you manage code access.'
  }
  if (
    lower.includes('invalid token') ||
    lower.includes('bad credentials') ||
    lower.includes('expired token') ||
    lower.includes('token expired')
  ) {
    return 'Paste a new code access key from GitHub or GitLab, then save again.'
  }
  if (code === 409 || lower.includes('already exists')) {
    return 'Remove the old code access entry first or choose the other site. Code access for this GitHub or GitLab choice already exists.'
  }
  if (code === 422 || lower.includes('invalid host') || lower.includes('invalid provider')) {
    return validationGuidance(lower)
  }
  if (
    lower.includes('not configured') ||
    lower.includes('provider is not configured') ||
    lower.includes('provider not configured')
  ) {
    return 'Ask an owner or admin to check code access settings, then try again.'
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return 'Wait a minute, then try again. Forge is receiving too many code access requests right now.'
  }
  if (code != null && code >= 500) {
    if (action === 'load') {
      return 'Refresh Settings to load code access. If it still fails, ask an owner or admin to check code access settings.'
    }
    return `Refresh Settings, then ${retry}. If it still fails, ask an owner or admin to check code access settings.`
  }
  if (isNetworkError(error)) {
    return connectionMessage(action)
  }

  if (action === 'load') {
    return 'Refresh Settings to load code access. If it still fails, ask an owner or admin to check code access settings.'
  }
  return `Try to ${retry}. If it still fails, ask an owner or admin to check code access settings.`
}
