export type AgentPluginErrorAction = 'load' | 'save'

function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function statusCode(err: unknown): number | null {
  const match = errorText(err).match(/\b(?:HTTP|API)\s+(\d{3})\b/i)
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

function prefix(action: AgentPluginErrorAction): string {
  return action === 'load'
    ? 'Agent tools could not be loaded.'
    : 'Tool change was not saved. The switch was returned to its previous setting.'
}

export function agentPluginErrorMessage(action: AgentPluginErrorAction, err: unknown): string {
  const code = statusCode(err)
  const base = prefix(action)

  if (code === 401) {
    return `${base} Sign in again, then reopen this agent.`
  }
  if (code === 403) {
    return `${base} Ask a workspace owner or admin to give you permission for this agent.`
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
    return `${base} The platform is temporarily unavailable. Try again in a few minutes.`
  }
  if (isNetworkError(err)) {
    return `${base} Check your connection, then try again.`
  }
  if (errorText(err).toLowerCase().includes('ok: false')) {
    return `${base} The platform did not return a usable tools list. Refresh the page or ask an admin to check the workspace tools.`
  }

  return `${base} Try again. If it still fails, ask an admin to check this agent's tool settings.`
}
