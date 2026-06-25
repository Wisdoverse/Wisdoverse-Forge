// Shared SSE primitives for the provider+prompt stream (POST /agents/:id/prompt
// returns `text/event-stream`). Kept in the `shared` layer so both the chat view
// (`features/chat/useChatStream`) and the quick-message composer
// (`features/agents`) can consume provider streams without a cross-feature import.

export interface SseFrame {
  event: string
  data: unknown
}

/** Parse a single SSE frame (two-newline terminated). Returns null on incomplete input. */
export function parseSseFrame(chunk: string): SseFrame | null {
  if (!chunk.includes('\n\n')) return null
  const lines = chunk.split('\n').filter(Boolean)
  let event = 'message'
  let data: unknown = null
  for (const line of lines) {
    if (line.startsWith('event:')) event = line.slice(6).trim()
    else if (line.startsWith('data:')) {
      const raw = line.slice(5).trim()
      try {
        data = JSON.parse(raw)
      } catch {
        data = raw
      }
    }
  }
  return { event, data }
}

export function asRecord(value: unknown): Record<string, unknown> {
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>
  }
  return {}
}

function messageDetail(body: Record<string, unknown>): string | null {
  const detail =
    (typeof body.error === 'string' && body.error.trim()) ||
    (typeof body.message === 'string' && body.message.trim()) ||
    null
  return detail || null
}

export function isAbortError(error: unknown): boolean {
  return (
    error !== null &&
    typeof error === 'object' &&
    'name' in error &&
    (error as { name?: unknown }).name === 'AbortError'
  )
}

export function chatStreamHttpErrorMessage(
  status: number,
  body: Record<string, unknown> = {}
): string {
  const detail = messageDetail(body)

  if (status === 401) {
    return 'Sign in again, then open this agent chat and resend the message. Your sign-in expired.'
  }
  if (status === 403) {
    return 'Ask an owner or admin to update your team space access before using this agent chat. You do not have access to this agent or team space.'
  }
  if (status === 404) {
    return 'Go back to Agents, choose an active agent, then open Chat again. This agent could not be found.'
  }
  if (status === 409) {
    return chatStreamConflictMessage(detail)
  }
  if (status === 429) {
    return 'Wait a moment, then resend the message. This agent is receiving too many messages right now.'
  }
  if (status >= 500) {
    return 'Wait a few minutes, then resend the message. Forge could not send this chat message right now. If it still fails, ask an owner or admin to check this agent chat and agent status.'
  }

  return 'Go back to Agents, choose this agent again, then open Chat and resend the message. This message was not sent.'
}

function chatStreamConflictMessage(detail: string | null): string {
  const normalized = detail?.toLowerCase() ?? ''
  if (normalized.includes('busy') || normalized.includes('working')) {
    return 'Wait for the current reply to finish, then resend the message. This agent is already working.'
  }
  return 'Open this chat again, check the latest message, then resend the message. This conversation changed while the message was sending.'
}

export function chatStreamRequestErrorMessage(error: unknown): string {
  if (isAbortError(error)) return ''
  return 'Check your connection, then resend the message. Forge could not connect while sending this message.'
}

export function chatStreamReadErrorMessage(error: unknown): string {
  if (isAbortError(error)) return ''
  return 'Check that the agent is still online, then resend the message. The reply stopped before it finished.'
}

export function chatStreamEventErrorMessage(detail: unknown): string {
  const text = typeof detail === 'string' ? detail.toLowerCase() : ''
  if (text.includes('rate') || text.includes('limit') || text.includes('too many')) {
    return 'Wait a moment, then resend the message. This agent is receiving too many messages right now.'
  }
  if (
    text.includes('permission') ||
    text.includes('forbidden') ||
    text.includes('role required') ||
    text.includes('unauthorized') ||
    text.includes('authorization') ||
    text.includes('bearer')
  ) {
    return 'Ask an owner or admin to update your team space access before using this agent chat. You do not have access to this agent chat.'
  }
  if (text.includes('context')) {
    return 'This chat has too many old messages. Clear chat only if those messages are no longer useful, then send the message again.'
  }
  return 'Resend the message. The agent could not finish this reply. If it still fails, ask an owner or admin to check this agent chat.'
}

export interface PromptStreamOutcome {
  ok: boolean
  error?: string
}

/**
 * Consume a provider prompt SSE `Response` for the quick-message composer:
 * the reply is rendered elsewhere (history view), so this discards frames and
 * only reports whether the send succeeded.
 *
 * The user turn is persisted server-side BEFORE the stream is built, so a 200
 * with a started stream means the message was accepted; an EOF or a caller abort
 * is therefore success. A non-2xx (e.g. the model-vision gate), a connection
 * failure, or an in-stream `error` event is a failure with a user-facing message.
 */
export async function consumePromptStream(resp: Response): Promise<PromptStreamOutcome> {
  if (!resp.ok) {
    const body = (await resp.json().catch(() => ({}))) as Record<string, unknown>
    return { ok: false, error: chatStreamHttpErrorMessage(resp.status, body) }
  }
  if (!resp.body) {
    // No stream body but a 2xx — the message was accepted; nothing to read.
    return { ok: true }
  }

  const reader = resp.body.getReader()
  const decoder = new TextDecoder()
  let buf = ''
  while (true) {
    let chunk: ReadableStreamReadResult<Uint8Array>
    try {
      chunk = await reader.read()
    } catch (error) {
      // Abort (composer closed) → the message was already sent; treat as success.
      if (isAbortError(error)) return { ok: true }
      return { ok: false, error: chatStreamReadErrorMessage(error) }
    }
    if (chunk.done) break
    buf += decoder.decode(chunk.value, { stream: true })
    while (buf.includes('\n\n')) {
      const end = buf.indexOf('\n\n') + 2
      const raw = buf.slice(0, end)
      buf = buf.slice(end)
      const frame = parseSseFrame(raw)
      if (!frame) continue
      if (frame.event === 'error') {
        const payload = asRecord(frame.data)
        return { ok: false, error: chatStreamEventErrorMessage(payload.message) }
      }
      // All other frames (message_start/delta/message_stop/unknown) are rendered
      // by the history view; the composer only cares about errors and completion.
    }
  }
  return { ok: true }
}
