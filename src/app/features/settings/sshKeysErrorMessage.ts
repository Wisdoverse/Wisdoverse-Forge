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

function baseMessage(action: SshKeyAction): string {
  if (action === 'save') return 'git@ repository access could not be saved.'
  if (action === 'remove') return 'git@ repository access could not be removed.'
  return 'git@ repository access could not be loaded.'
}

export function sshKeysErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const base = baseMessage(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `${base} Your sign-in expired. Sign in again, then open Settings and try git@ repository access again.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return `${base} Ask an owner or admin for access to manage git@ repository access.`
  }
  if (
    lower.includes('add a name') ||
    lower.includes('add a label') ||
    lower.includes('access name')
  ) {
    return `${base} Add a name for this access, then save again.`
  }
  if (
    lower.includes('invalid public key') ||
    lower.includes('invalid ssh key') ||
    lower.includes('bad key') ||
    lower.includes('private key') ||
    lower.includes('openssh private key') ||
    lower.includes('begin private key')
  ) {
    return `${base} Paste only the shareable one-line SSH key that starts with ssh-ed25519 or ssh-rsa, then save again. Do not paste a private key block.`
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return `${base} This public SSH key line is already saved. Choose the saved access or remove the old one first.`
  }
  if (
    lower.includes('shareable ssh line') ||
    lower.includes('shareable ssh key') ||
    lower.includes('public key') ||
    lower.includes('ssh key')
  ) {
    return `${base} Paste the public SSH key line that starts with ssh-ed25519 or ssh-rsa, then save again.`
  }
  if (code === 422 || lower.includes('required') || lower.includes('missing')) {
    return `${base} Check the access name and public SSH key line, then try again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} Forge is receiving too many git@ repository access requests right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} Refresh Settings, then try again. If it still fails, ask an owner or admin to check git@ repository access settings.`
  }
  if (isNetworkError(error)) {
    return `${base} Forge could not connect while opening git@ repository access. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check git@ repository access settings.`
}
