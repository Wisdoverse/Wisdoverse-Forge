type SshKeyAction = 'load' | 'save' | 'remove'

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

function actionFromText(text: string): SshKeyAction {
  if (/\b(save|saving|saved|create|created|update|updated)\b/i.test(text)) return 'save'
  if (/\b(delete|deleted|remove|removed|removing)\b/i.test(text)) return 'remove'
  return 'load'
}

function baseMessage(action: SshKeyAction): string {
  if (action === 'save') return 'Repository SSH access could not be saved.'
  if (action === 'remove') return 'Repository SSH access could not be removed.'
  return 'Repository SSH access could not be loaded.'
}

export function sshKeysErrorMessage(error: unknown): string {
  const text = errorText(error)
  const lower = text.toLowerCase()
  const code = statusCode(error)
  const action = actionFromText(text)
  const base = baseMessage(action)

  if (code === 401 || lower.includes('sign in again') || lower.includes('unauthorized')) {
    return `${base} Sign in again, then open Settings and try repository SSH access again.`
  }
  if (code === 403 || lower.includes('permission') || lower.includes('forbidden')) {
    return `${base} Ask an owner or admin for access to manage repository SSH access.`
  }
  if (
    lower.includes('invalid public key') ||
    lower.includes('invalid ssh key') ||
    lower.includes('bad key') ||
    lower.includes('private key') ||
    lower.includes('openssh private key') ||
    lower.includes('begin private key')
  ) {
    return `${base} Paste only the shareable public line that starts with ssh-ed25519 or ssh-rsa, then save again.`
  }
  if (code === 409 || lower.includes('already exists') || lower.includes('duplicate')) {
    return `${base} This public line already exists. Choose the saved access or remove the old one first.`
  }
  if (code === 422 || lower.includes('required') || lower.includes('missing')) {
    return `${base} Check the access name and public SSH line, then try again.`
  }
  if (code === 429 || lower.includes('busy') || lower.includes('too many')) {
    return `${base} The service is busy. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The repository SSH access service is temporarily unavailable. Try again. If it still fails, ask an owner to check repository SSH access settings.`
  }
  if (isNetworkError(error)) {
    return `${base} The app could not reach the service. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner to check repository SSH access settings.`
}
