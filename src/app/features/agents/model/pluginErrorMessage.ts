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
const LOAD_TOOLS_STEP = 'Go back to Agents, choose this agent again, then open Tools.'
const LOAD_TOOLS_AFTER_ACTION = 'go back to Agents, choose this agent again, then open Tools.'
const LOAD_TOOLS_AGAIN_AFTER_ACTION =
  'go back to Agents, choose this agent again, then open Tools again.'
const RETRY_TOOL_CHANGE_STEP =
  'Go back to Agents, choose this agent again, then open Tools and try the tool change again.'
const CHOOSE_CURRENT_TOOL_STEP =
  'Go back to Agents, choose this agent again, then open Tools and choose the current tool again.'

function loadPrefix(): string {
  return LOAD_TOOLS_STEP
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
      ? `Sign in again, then ${LOAD_TOOLS_AFTER_ACTION}`
      : saveMessage(
          'Sign in again, then go back to Agents, choose this agent again, and try the tool change again.',
          ''
        )
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
          CHOOSE_CURRENT_TOOL_STEP,
          'This agent or tool may have been changed by someone else.'
        )
  }
  if (code === 409) {
    return action === 'load'
      ? `Wait a moment, then ${LOAD_TOOLS_AGAIN_AFTER_ACTION} Another change is still being saved.`
      : saveMessage(
          'Wait a moment, then try the tool change again.',
          'Another change is still being saved.'
        )
  }
  if (code === 429) {
    return action === 'load'
      ? `Wait a minute, then ${LOAD_TOOLS_AGAIN_AFTER_ACTION} Too many requests are happening right now.`
      : saveMessage(
          'Wait a minute, then try the tool change again.',
          'Too many requests are happening right now.'
        )
  }
  if (code != null && code >= 500) {
    return action === 'load'
      ? `Wait a few minutes, then ${LOAD_TOOLS_AGAIN_AFTER_ACTION} Forge could not finish this tool request right now. If it still fails, ask an owner or admin to check this agent's tool list.`
      : saveMessage(
          'Wait a few minutes, then try the tool change again.',
          "Forge could not finish this tool request right now. If it still fails, ask an owner or admin to check this agent's tool list."
        )
  }
  if (isNetworkError(err)) {
    return action === 'load'
      ? `Check your connection, then ${LOAD_TOOLS_AGAIN_AFTER_ACTION} Forge could not connect while checking this agent's tools.`
      : saveMessage(
          'Check your connection, then try the tool change again.',
          "Forge could not connect while checking this agent's tools."
        )
  }
  if (text.includes('ok: false')) {
    return action === 'load'
      ? `${base} If it still fails, ask an owner or admin to check team space tools.`
      : saveMessage(
          RETRY_TOOL_CHANGE_STEP,
          "Forge could not read this agent's tool list. If it still fails, ask an owner or admin to check team space tools."
        )
  }

  return action === 'load'
    ? `${base} If it still fails, ask an owner or admin to check this agent's tool list.`
    : saveMessage(
        RETRY_TOOL_CHANGE_STEP,
        "If it still fails, ask an owner or admin to check this agent's tool list."
      )
}
