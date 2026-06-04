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

export function createAgentWorkLaneErrorMessage(error: unknown): string {
  const base = 'Work lane was not created.'
  const text = errorText(error).toLowerCase()
  const code = statusCode(error)

  if (code === 401 || text.includes('unauthorized') || text.includes('sign in again')) {
    return `${base} Sign in again, reopen New Agent, and try creating the lane again.`
  }
  if (code === 403 || text.includes('forbidden') || text.includes('permission')) {
    return `${base} Ask a workspace owner or admin to let you manage this project's work lanes.`
  }
  if (code === 404) {
    return `${base} Refresh the page; this project may have changed or been removed.`
  }
  if (
    code === 409 ||
    text.includes('already exists') ||
    text.includes('already exist') ||
    text.includes('duplicate')
  ) {
    return `${base} A default lane may already exist. Refresh the project, then choose the existing lane.`
  }
  if (code === 422 || text.includes('validation')) {
    return `${base} Check that a project is selected, then try again.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many lane changes are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The platform is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(error)) {
    return `${base} Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this project.`
}
