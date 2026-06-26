import { useCallback, useEffect, useRef } from 'react'
import { useChatStore } from '@app/shared/model/chat.store'
import { getAgentApi } from '@app/shared/api/legacy'
import {
  asRecord,
  chatStreamEventErrorMessage,
  chatStreamHttpErrorMessage,
  chatStreamReadErrorMessage,
  chatStreamRequestErrorMessage,
  parseSseFrame,
} from '@app/shared/api/promptStream'
/** React hook: `send(content)` streams LLM reply; `abort()` cancels it. */
export function useChatStream(agentId: string) {
  const onUserMessage = useChatStore((s) => s.onUserMessage)
  const onMessageStart = useChatStore((s) => s.onMessageStart)
  const onDelta = useChatStore((s) => s.onDelta)
  const onMessageStop = useChatStore((s) => s.onMessageStop)
  const onStreamError = useChatStore((s) => s.onStreamError)
  const controllerRef = useRef<AbortController | null>(null)

  const send = useCallback(
    async (content: string) => {
      controllerRef.current?.abort()
      const controller = new AbortController()
      controllerRef.current = controller

      // Optimistic append: user's prompt visible immediately — backend persists
      // via POST /prompt but never emits a `user_message` SSE frame.
      // The local uuid won't match the DB row's uuid; on next `loadMessages`
      // the canonical row replaces it. Brief visual dedup is acceptable.
      const localUserId = crypto.randomUUID()
      onUserMessage({ id: localUserId, agentId, content })

      const api = getAgentApi()
      let resp: Response
      try {
        resp = await api.streamPrompt(agentId, content, [], controller.signal)
      } catch (e) {
        const message = chatStreamRequestErrorMessage(e)
        if (message) onStreamError(message)
        return
      }

      if (!resp.ok) {
        const body = (await resp.json().catch(() => ({}))) as Record<string, unknown>
        onStreamError(chatStreamHttpErrorMessage(resp.status, body))
        return
      }

      if (!resp.body) {
        onStreamError(
          'The agent did not send a reply. Go back to Agents, choose this agent again, then open Chat and resend the message.'
        )
        return
      }

      const reader = resp.body.getReader()
      const decoder = new TextDecoder()
      let buf = ''
      let currentId: string | null = null
      let terminalReason: string | null = null

      while (true) {
        let chunk: ReadableStreamReadResult<Uint8Array>
        try {
          chunk = await reader.read()
        } catch (e) {
          const message = chatStreamReadErrorMessage(e)
          if (message) onStreamError(message)
          return
        }
        if (chunk.done) break
        buf += decoder.decode(chunk.value, { stream: true })

        while (buf.includes('\n\n')) {
          const end = buf.indexOf('\n\n') + 2
          const raw = buf.slice(0, end)
          buf = buf.slice(end)
          const frame = parseSseFrame(raw)
          if (!frame) continue
          const payload = asRecord(frame.data)
          switch (frame.event) {
            case 'message_start': {
              currentId = (payload.message_id as string) ?? null
              const model = typeof payload.model === 'string' ? payload.model : undefined
              if (currentId) onMessageStart({ id: currentId, agentId, model })
              break
            }
            case 'delta': {
              const text = typeof payload.text === 'string' ? payload.text : ''
              if (currentId && text) onDelta(currentId, text)
              break
            }
            case 'message_stop': {
              if (currentId) {
                const reason =
                  typeof payload.finish_reason === 'string' ? payload.finish_reason : 'stop'
                const tokensIn =
                  typeof payload.tokens_in === 'number' ? payload.tokens_in : undefined
                const tokensOut =
                  typeof payload.tokens_out === 'number' ? payload.tokens_out : undefined
                onMessageStop(currentId, reason, tokensIn, tokensOut)
                terminalReason = reason
              }
              currentId = null
              break
            }
            case 'error': {
              onStreamError(chatStreamEventErrorMessage(payload.message))
              currentId = null
              break
            }
          }
        }
      }

      // If stream ended without a terminal `message_stop` but we DID start a
      // message, treat it as interrupted so the active row isn't left hanging.
      if (currentId && !terminalReason) {
        onMessageStop(currentId, 'interrupted')
      }
    },
    [agentId, onUserMessage, onMessageStart, onDelta, onMessageStop, onStreamError]
  )

  const abort = useCallback(() => {
    const active = controllerRef.current
    if (!active) return
    active.abort()
    controllerRef.current = null
    const api = getAgentApi()
    void api.interruptPrompt(agentId)
  }, [agentId])

  // Unmount / agent-switch cleanup: abort any in-flight fetch so frames from
  // the previous stream don't race the new one into the shared store.
  useEffect(
    () => () => {
      controllerRef.current?.abort()
      controllerRef.current = null
    },
    [agentId]
  )

  return { send, abort }
}
