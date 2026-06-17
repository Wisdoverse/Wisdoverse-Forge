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

const TOOL_SWITCH_REVERTED = 'The switch was returned to its previous setting.'

function loadPrefix(): string {
  return 'Refresh this agent page to load tools.'
}

function saveMessage(firstStep: string, detail: string): string {
  return `${firstStep} ${TOOL_SWITCH_REVERTED} ${detail}`.trim()
}

export function agentPluginErrorMessage(action: AgentPluginErrorAction, err: unknown): string {
  const code = statusCode(err)
  const base = loadPrefix()
  const text = structuredErrorText(err).toLowerCase()

  if (code === 401) {
    return action === 'load'
      ? `${base} Sign in again, then reopen this agent.`
      : saveMessage('Sign in again, then reopen this agent and try the tool change again.', '')
  }
  if (code === 403) {
    return action === 'load'
      ? `${base} Ask an owner or admin to give you access to this agent's tools.`
      : saveMessage("Ask an owner or admin to give you access to this agent's tools.", '')
  }
  if (code === 404) {
    return action === 'load'
      ? `${base} This agent or tool may have been changed by someone else.`
      : saveMessage(
          'Refresh this agent page, then choose the current tool again.',
          'This agent or tool may have been changed by someone else.'
        )
  }
  if (code === 409) {
    return action === 'load'
      ? `${base} Another change is still being saved. Wait a moment, then try again.`
      : saveMessage(
          'Wait a moment, then try the tool change again.',
          'Another change is still being saved.'
        )
  }
  if (code === 429) {
    return action === 'load'
      ? `${base} Too many requests are happening right now. Wait a minute, then try again.`
      : saveMessage(
          'Wait a minute, then try the tool change again.',
          'Too many requests are happening right now.'
        )
  }
  if (code != null && code >= 500) {
    return action === 'load'
      ? `${base} Forge could not finish this tool request right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check this agent's tool setup.`
      : saveMessage(
          'Wait a few minutes, then try the tool change again.',
          "Forge could not finish this tool request right now. If it still fails, ask an owner or admin to check this agent's tool setup."
        )
  }
  if (isNetworkError(err)) {
    return action === 'load'
      ? `${base} Forge could not connect while checking this agent's tools. Check your connection, then refresh this agent page again.`
      : saveMessage(
          'Check your connection, then try the tool change again.',
          "Forge could not connect while checking this agent's tools."
        )
  }
  if (text.includes('ok: false')) {
    return action === 'load'
      ? `${base} If it still fails, ask an owner or admin to check team space tools.`
      : saveMessage(
          'Refresh this agent page, then try the tool change again.',
          "Forge could not read this agent's tool list. If it still fails, ask an owner or admin to check team space tools."
        )
  }

  return action === 'load'
    ? `${base} Try again. If it still fails, ask an owner or admin to check this agent's tool setup.`
    : saveMessage(
        'Refresh this agent page, then try the tool change again.',
        "If it still fails, ask an owner or admin to check this agent's tool setup."
      )
}
