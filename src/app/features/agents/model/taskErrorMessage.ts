function rawErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function statusCode(err: unknown): number | null {
  const match = rawErrorMessage(err).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(err: unknown): boolean {
  const text = rawErrorMessage(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function agentTasksErrorMessage(err: unknown): string {
  const base = 'This agent task list could not be loaded.'
  const code = statusCode(err)
  const text = rawErrorMessage(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return `${base} Sign in again, then reopen this agent.`
  }
  if (code === 403 || text.includes('forbidden')) {
    return `${base} Ask an owner or admin to give you access to this agent or its task queue.`
  }
  if (code === 404) {
    return `${base} Refresh the page; this agent may have changed or been removed.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many task requests are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} Forge could not load this task list right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check this agent's task setup.`
  }
  if (isNetworkError(err)) {
    return `${base} Forge could not connect while loading this task list. Check your connection, then try again.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this agent's task setup.`
}
