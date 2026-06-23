export type ChatErrorAction = 'load' | 'clear'

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

function baseMessage(action: ChatErrorAction): string {
  return action === 'load'
    ? 'Check conversation again to load the chat history.'
    : 'Chat was not cleared.'
}

function serviceRecoveryMessage(action: ChatErrorAction): string {
  return action === 'load'
    ? 'Wait a few minutes, then choose Check conversation again. Forge could not load this conversation right now. If it still fails, ask an owner or admin to check this agent chat.'
    : 'Wait a few minutes, then clear chat again if you still want to remove the messages. Forge could not update this chat right now. If it still fails, ask an owner or admin to check this agent chat.'
}

function networkRecoveryMessage(action: ChatErrorAction): string {
  return action === 'load'
    ? 'Check your connection, then choose Check conversation again. Forge could not connect while loading this conversation.'
    : 'Check your connection, then clear chat again. Forge could not connect while clearing this chat.'
}

function fallbackRecoveryMessage(action: ChatErrorAction): string {
  return action === 'load'
    ? 'Choose Check conversation again. If it still fails, ask an owner or admin to check this agent chat.'
    : 'Clear chat again if you still want to remove the messages. If it still fails, ask an owner or admin to check this agent chat.'
}

export function chatErrorMessage(action: ChatErrorAction, err: unknown): string {
  const base = baseMessage(action)
  const code = statusCode(err)
  const text = structuredErrorMessage(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized') || text.includes('authorization: bearer')) {
    return `${base} Sign in again, then reopen this chat.`
  }
  if (code === 403 || text.includes('forbidden') || text.includes('role required')) {
    return `${base} Ask an owner or admin to give you access to this agent.`
  }
  if (code === 404) {
    return `${base} Open Agents, choose this agent again, then open Chat. This agent or conversation may have changed.`
  }
  if (code === 409) {
    return action === 'load'
      ? `${base} Wait a moment, then choose Check conversation again. Another chat action is still saving.`
      : `${base} Wait a moment, then clear chat again if you still want to remove the messages. Another chat action is still saving.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return action === 'load'
      ? `${base} Wait a minute, then choose Check conversation again. Too many chat requests are happening right now.`
      : `${base} Wait a minute, then clear chat again if you still want to remove the messages. Too many chat requests are happening right now.`
  }
  if (code != null && code >= 500) {
    return `${base} ${serviceRecoveryMessage(action)}`
  }
  if (isNetworkError(err)) {
    return `${base} ${networkRecoveryMessage(action)}`
  }
  if (text.includes('ok: false')) {
    return `${base} Choose Check conversation again. Forge could not read this conversation.`
  }

  return `${base} ${fallbackRecoveryMessage(action)}`
}
