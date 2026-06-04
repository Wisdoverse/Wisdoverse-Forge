function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function statusCode(err: unknown): number | null {
  const match = errorText(err).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(err: unknown): boolean {
  const text = errorText(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function agentGroupErrorMessage(err: unknown): string {
  const base = 'Work lane was not created.'
  const code = statusCode(err)
  const text = errorText(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return `${base} Sign in again, then choose the project and try again.`
  }
  if (code === 403 || text.includes('forbidden')) {
    return `${base} Ask a workspace owner or admin to let you manage this project's work lanes.`
  }
  if (code === 404) {
    return `${base} Refresh the page; this project may have changed or been removed.`
  }
  if (code === 409) {
    return `${base} A lane with this name may already exist. Use a different name, then try again.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many lane changes are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} The platform is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(err)) {
    return `${base} Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this project.`
}
