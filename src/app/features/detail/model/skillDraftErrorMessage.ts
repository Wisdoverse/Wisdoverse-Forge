function errorText(error: unknown): string {
  if (error instanceof Error) return error.message
  return typeof error === 'string' ? error : ''
}

function statusCode(error: unknown): number | null {
  if (error && typeof error === 'object' && 'statusCode' in error) {
    const statusCode = (error as { statusCode?: unknown }).statusCode
    if (typeof statusCode === 'number') return statusCode
  }

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
    text.includes('load failed')
  )
}

export function skillDraftErrorMessage(error: unknown): string {
  const base = 'Skill was not published.'
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('unauthorized') || text.includes('sign in again')) {
    return `${base} Sign in again, reopen this task, and publish the skill again.`
  }
  if (code === 403 || text.includes('forbidden') || text.includes('permission')) {
    return `${base} Ask a workspace owner or admin to let you create workspace skills.`
  }
  if (code === 404) {
    return `${base} Refresh the task; the workspace or skill service may have changed.`
  }
  if (
    code === 409 ||
    text.includes('already exists') ||
    text.includes('already exist') ||
    text.includes('duplicate')
  ) {
    return `${base} A skill with this name may already exist. Rename it, then publish again.`
  }
  if (code === 422 || text.includes('validation')) {
    return `${base} Check the name, trigger words, and reusable instructions, then publish again.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many skill changes are happening right now. Wait a minute, then publish again.`
  }
  if (code != null && code >= 500) {
    return `${base} The skill service is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(error)) {
    return `${base} Check your connection, then publish again.`
  }

  return `${base} Review the draft and try again. If it still fails, ask an owner or admin to check workspace skills.`
}
