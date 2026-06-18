type SshKeyAction = 'load' | 'save' | 'remove'

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

function actionFromText(text: string): SshKeyAction {
  const lower = text.toLowerCase()
  if (/\b(save|saving|saved|create|created|update|updated)\b/i.test(text)) return 'save'
  if (/\b(delete|deleted|remove|removed|removing)\b/i.test(text)) return 'remove'
  if (
    lower.includes('add a name') ||
    lower.includes('add a label') ||
    lower.includes('shareable ssh') ||
    lower.includes('public ssh key') ||
    lower.includes('public key') ||
    lower.includes('private key') ||
    lower.includes('already exists') ||
    lower.includes('duplicate') ||
    lower.includes('required')
  ) {
    return 'save'
  }
  return 'load'
}

function retryAction(action: SshKeyAction): string {
  if (action === 'save') return 'save this SSH code access again'
  if (action === 'remove') return 'remove this SSH code access again'
  return 'refresh Settings to load SSH code access'
}

function connectionMessage(action: SshKeyAction): string {
  if (action === 'load') {
    return 'Check your connection, then refresh Settings to load SSH code access. Forge could not connect while opening SSH code access.'
  }
  if (action === 'remove') {
    return 'Check your connection, then remove this SSH code access again. The removal did not finish.'
  }
  return 'Check your connection, then save this SSH code access again. The save did not finish.'
}

export function sshKeysErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const retry = retryAction(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `Sign in again, then ${retry}. Your sign-in expired.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return 'Ask an owner or admin for access to manage SSH code access.'
  }
  if (isNetworkError(error)) {
    return connectionMessage(action)
  }
  if (
    lower.includes('add a name') ||
    lower.includes('add a label') ||
    lower.includes('access name')
  ) {
    return 'Add a name for this access, then save again.'
  }
  if (
    lower.includes('invalid public key') ||
    lower.includes('invalid ssh key') ||
    lower.includes('bad key') ||
    lower.includes('private key') ||
    lower.includes('openssh private key') ||
    lower.includes('begin private key')
  ) {
    return 'Paste only the safe one-line public key from the .pub file, then save again. Do not paste a private key block.'
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return 'Choose the saved access or remove the old one first. This safe public key line is already saved.'
  }
  if (
    lower.includes('shareable ssh line') ||
    lower.includes('shareable ssh key') ||
    lower.includes('public key') ||
    lower.includes('ssh key')
  ) {
    return 'Paste the safe public key line from the .pub file, then save again.'
  }
  if (code === 422 || lower.includes('required') || lower.includes('missing')) {
    return 'Check the access name and safe public key line, then try again.'
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return 'Wait a minute, then try again. Forge is receiving too many SSH code access requests right now.'
  }
  if (code != null && code >= 500) {
    if (action === 'load') {
      return 'Refresh Settings to load SSH code access. If it still fails, ask an owner or admin to check SSH code access settings.'
    }
    return `Refresh Settings, then ${retry}. If it still fails, ask an owner or admin to check SSH code access settings.`
  }

  if (action === 'load') {
    return 'Refresh Settings to load SSH code access. If it still fails, ask an owner or admin to check SSH code access settings.'
  }

  return `Try to ${retry}. If it still fails, ask an owner or admin to check SSH code access settings.`
}
