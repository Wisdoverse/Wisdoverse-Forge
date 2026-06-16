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
    ? 'Retry conversation to load conversation history.'
    : 'Chat was not cleared.'
}

function serviceRecoveryMessage(action: ChatErrorAction): string {
  return action === 'load'
    ? 'Wait a few minutes, then choose Retry conversation again. Forge could not load this conversation right now. If it still fails, ask an owner or admin to check chat setup.'
    : 'Wait a few minutes, then clear chat again if you still want to remove the messages. Forge could not update this chat right now. If it still fails, ask an owner or admin to check chat setup.'
}

function networkRecoveryMessage(action: ChatErrorAction): string {
  return action === 'load'
    ? 'Check your connection, then choose Retry conversation again. Forge could not connect while loading this conversation.'
    : 'Check your connection, then clear chat again. Forge could not connect while clearing this chat.'
}

export function chatErrorMessage(action: ChatErrorAction, err: unknown): string {
  const base = baseMessage(action)
  const code = statusCode(err)
  const text = structuredErrorMessage(err).toLowerCase()

  if (code === 401 || text.includes('unauthorized')) {
    return `${base} Sign in again, then reopen this chat.`
  }
  if (code === 403 || text.includes('forbidden')) {
    return `${base} Ask an owner or admin to give you access to this agent.`
  }
  if (code === 404) {
    return `${base} Refresh the page; this agent or conversation may have changed.`
  }
  if (code === 409) {
    return `${base} Another chat action is still saving. Wait a moment, then try again.`
  }
  if (code === 429 || text.includes('rate limit') || text.includes('too many')) {
    return `${base} Too many chat requests are happening right now. Wait a minute, then try again.`
  }
  if (code != null && code >= 500) {
    return `${base} ${serviceRecoveryMessage(action)}`
  }
  if (isNetworkError(err)) {
    return `${base} ${networkRecoveryMessage(action)}`
  }
  if (text.includes('ok: false')) {
    return `${base} Refresh the chat, then try again. Forge could not read this conversation.`
  }

  return `${base} Try again. If it still fails, ask an owner or admin to check this agent's chat setup.`
}
