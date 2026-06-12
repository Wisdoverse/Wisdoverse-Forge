export type AgentPluginErrorAction = 'load' | 'save'

function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function structuredErrorText(err: unknown): string {
  if (!err || typeof err !== 'object') return errorText(err)
  for (const key of ['serverError', 'detail', 'error', 'message', 'reason'] as const) {
    const value = (err as Record<string, unknown>)[key]
    if (typeof value === 'string' && value.trim()) return value
  }
  return errorText(err)
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

  const match = structuredErrorText(err).match(/\b(?:HTTP|API|Server error)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(err: unknown): boolean {
  const text = structuredErrorText(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('load failed')
  )
}

function prefix(action: AgentPluginErrorAction): string {
  return action === 'load'
    ? 'Agent tools could not be loaded.'
    : 'Tool change was not saved. The switch was returned to its previous setting.'
}

export function agentPluginErrorMessage(action: AgentPluginErrorAction, err: unknown): string {
  const code = statusCode(err)
  const base = prefix(action)
  const text = structuredErrorText(err).toLowerCase()

  if (code === 401) {
    return `${base} Sign in again, then reopen this agent.`
  }
  if (code === 403) {
    return `${base} Ask an owner or admin to give you access to this agent's tools.`
  }
  if (code === 404) {
    return `${base} Refresh the page; this agent or tool may have been changed by someone else.`
  }
  if (code === 409) {
    return `${base} Another change is still being saved. Wait a moment, then try again.`
  }
  if (code === 429) {
    return `${base} Too many requests are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} Forge could not finish this tool request right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check this agent's tool setup.`
  }
  if (isNetworkError(err)) {
    return `${base} Forge could not connect while checking this agent's tools. Check your connection, then try again.`
  }
  if (text.includes('ok: false')) {
    return `${base} Forge could not read this agent's tool list. Refresh the page. If it still fails, ask an owner or admin to check workspace tools.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this agent's tool setup.`
}
