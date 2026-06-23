function rawErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function structuredErrorMessage(err: unknown): string {
  if (!err || typeof err !== 'object') return rawErrorMessage(err)
  for (const key of ['serverError', 'detail', 'error', 'message', 'reason'] as const) {
    const value = (err as Record<string, unknown>)[key]
    if (typeof value === 'string' && value.trim()) return value
  }
  return rawErrorMessage(err)
}

function statusCode(err: unknown): number | null {
  if (err && typeof err === 'object') {
    for (const key of ['statusCode', 'status', 'code'] as const) {
      const value = (err as Record<string, unknown>)[key]
      if (typeof value === 'number' && Number.isFinite(value)) return value
      if (typeof value === 'string' && /^\d{3}$/.test(value.trim())) {
        return Number.parseInt(value, 10)
      }
    }
  }

  const match = structuredErrorMessage(err).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(err: unknown): boolean {
  const text = structuredErrorMessage(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

export function agentTasksErrorMessage(err: unknown): string {
  const code = statusCode(err)
  const text = structuredErrorMessage(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return 'Sign in again, then reopen this agent to load its work list.'
  }
  if (code === 403 || text.includes('forbidden') || text.includes('role required')) {
    return "Ask an owner or admin to give you access to this agent's work list."
  }
  if (code === 404) {
    return 'Open Agents, choose this agent again, then open Work to load the work list. This agent may have changed or been removed.'
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return 'Too many task requests are happening right now. Wait a minute, then open Work again from this agent.'
  }
  if (code != null && code >= 500) {
    return "Open Work again from this agent. If it still fails, ask an owner or admin to check this agent's work list."
  }
  if (isNetworkError(err)) {
    return 'Check your connection, then open Work again from this agent.'
  }

  return "Open Work again from this agent. If it still fails, ask an owner or admin to check this agent's work list."
}
