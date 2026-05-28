import { create } from 'zustand'
import type { ClaudeEvent } from '@shared/types/events'
import type { AgentMessageRow } from '@shared/types'
import { getAgentApi } from '@app/shared/api/legacy'
import { extractApiError } from '@app/shared/api/agent-api-types'

// ============================================================================
// Types
// ============================================================================

export interface ToolCall {
  toolUseId: string
  tool: string
  input: Record<string, unknown>
  output?: Record<string, unknown>
  success?: boolean
  duration?: number
}

export interface Turn {
  id: string
  prompt?: string
  toolCalls: ToolCall[]
  response?: string
  timestamp: number
}

interface ChatState {
  // Existing CLI-tool event-grouped model (unchanged):
  turns: Turn[]
  /** Set by fetchEvents — CLI event-grouping path (turn-based view). */
  loading: boolean
  error: string | null
  fetchEvents: (agentId: string) => Promise<void>
  reset: () => void

  // Provider+prompt streaming model:
  messages: AgentMessageRow[]
  streaming: boolean
  streamingMessageId: string | null
  /** Set by loadMessages — SSE/provider+prompt chat history path. */
  messagesLoading: boolean

  loadMessages: (agentId: string) => Promise<void>
  /** Optimistically append a user-role message before the assistant stream starts. */
  onUserMessage: (row: { id: string; agentId: string; content: string }) => void
  onMessageStart: (frame: { id: string; agentId: string; model?: string }) => void
  onDelta: (id: string, text: string) => void
  onMessageStop: (
    id: string,
    finishReason: string,
    tokensIn?: number | null,
    tokensOut?: number | null
  ) => void
  onStreamError: (message: string) => void
  clearMessages: (agentId: string) => Promise<void>
  resetMessages: () => void
}

// ============================================================================
// Turn Grouping
// ============================================================================

function groupEventsIntoTurns(events: ClaudeEvent[]): Turn[] {
  const turns: Turn[] = []
  let current: Turn | null = null

  // Events arrive newest-first from the API; reverse for chronological processing
  const sorted = [...events].sort((a, b) => a.timestamp - b.timestamp)

  for (const event of sorted) {
    switch (event.type) {
      case 'user_prompt_submit': {
        // Start a new turn
        if (current) turns.push(current)
        current = {
          id: event.id,
          prompt: event.prompt,
          toolCalls: [],
          timestamp: event.timestamp,
        }
        break
      }
      case 'pre_tool_use': {
        if (!current) {
          current = {
            id: event.id,
            toolCalls: [],
            timestamp: event.timestamp,
          }
        }
        current.toolCalls.push({
          toolUseId: event.toolUseId,
          tool: event.tool,
          input: event.toolInput,
        })
        break
      }
      case 'post_tool_use': {
        if (current) {
          const tc = current.toolCalls.find((t) => t.toolUseId === event.toolUseId)
          if (tc) {
            tc.output = event.toolResponse
            tc.success = event.success
            tc.duration = event.duration
          }
        }
        break
      }
      case 'stop': {
        if (!current) {
          current = {
            id: event.id,
            toolCalls: [],
            timestamp: event.timestamp,
          }
        }
        current.response = event.response
        turns.push(current)
        current = null
        break
      }
      default:
        // Ignore lifecycle events (session_start, session_end, etc.)
        break
    }
  }

  // Push any incomplete turn
  if (current) turns.push(current)

  return turns
}

// ============================================================================
// Store
// ============================================================================

const initialState = {
  turns: [] as Turn[],
  loading: false,
  error: null as string | null,
  messages: [] as AgentMessageRow[],
  streaming: false,
  streamingMessageId: null as string | null,
  messagesLoading: false,
}

export const useChatStore = create<ChatState>((set) => ({
  ...initialState,

  fetchEvents: async (agentId: string) => {
    set({ loading: true, error: null })
    try {
      const token = typeof window !== 'undefined' ? localStorage.getItem('af:auth:access') : null
      const res = await fetch(`/api/v1/agents/${encodeURIComponent(agentId)}/events?limit=200`, {
        headers: {
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
      })
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`)
      }
      const data = (await res.json()) as { ok: boolean; events: ClaudeEvent[] }
      if (!data.ok) {
        throw new Error('Server returned ok: false')
      }
      const turns = groupEventsIntoTurns(data.events)
      set({ turns, loading: false })
    } catch {
      set({ loading: false, error: 'Failed to load conversation history' })
    }
  },

  reset: () => set(initialState),

  loadMessages: async (agentId: string) => {
    set({ messagesLoading: true, error: null })
    try {
      const api = getAgentApi()
      const result = await api.fetchMessages(agentId, { limit: 50 })
      if (result.ok && result.messages) {
        set({ messages: result.messages, messagesLoading: false })
      } else {
        set({ messagesLoading: false, error: extractApiError(result, 'Failed to load messages') })
      }
    } catch (err) {
      console.error('loadMessages failed:', err)
      set({
        messagesLoading: false,
        error: err instanceof Error ? err.message : 'Failed to load messages',
      })
    }
  },

  onUserMessage: (row) =>
    set((s) => {
      // Idempotent against accidental double-fire (e.g., re-entrant send).
      if (s.messages.some((m) => m.id === row.id)) return s
      return {
        messages: [
          ...s.messages,
          {
            id: row.id,
            agentId: row.agentId,
            role: 'user' as const,
            content: row.content,
            tokensIn: null,
            tokensOut: null,
            model: null,
            finishReason: null,
            createdAt: new Date().toISOString(),
          },
        ],
      }
    }),

  onMessageStart: ({ id, agentId, model }) =>
    set((s) => {
      // Idempotent against duplicate start frames (SSE reconnect).
      if (s.messages.some((m) => m.id === id)) {
        return { streaming: true, streamingMessageId: id }
      }
      return {
        streaming: true,
        streamingMessageId: id,
        messages: [
          ...s.messages,
          {
            id,
            agentId,
            role: 'assistant' as const,
            content: '',
            tokensIn: null,
            tokensOut: null,
            model: model ?? null,
            finishReason: null,
            createdAt: new Date().toISOString(),
          },
        ],
      }
    }),

  onDelta: (id, text) =>
    set((s) => ({
      messages: s.messages.map((m) => (m.id === id ? { ...m, content: m.content + text } : m)),
    })),

  onMessageStop: (id, finishReason, tokensIn, tokensOut) =>
    set((s) => ({
      streaming: false,
      streamingMessageId: null,
      messages: s.messages.map((m) =>
        m.id === id
          ? {
              ...m,
              tokensIn: tokensIn ?? m.tokensIn,
              tokensOut: tokensOut ?? m.tokensOut,
              finishReason,
            }
          : m
      ),
    })),

  onStreamError: (msg) =>
    set((s) => ({
      streaming: false,
      streamingMessageId: null,
      error: msg,
      messages: s.streamingMessageId
        ? s.messages.map((m) =>
            m.id === s.streamingMessageId ? { ...m, finishReason: 'error' } : m
          )
        : s.messages,
    })),

  clearMessages: async (agentId: string) => {
    try {
      const api = getAgentApi()
      const result = await api.deleteMessages(agentId)
      if (result.ok) {
        set({ messages: [] })
      } else {
        set({ error: extractApiError(result, 'Failed to clear chat') })
      }
    } catch (err) {
      console.error('clearMessages failed:', err)
      set({ error: err instanceof Error ? err.message : 'Failed to clear messages' })
    }
  },

  resetMessages: () =>
    set({ messages: [], streaming: false, streamingMessageId: null, messagesLoading: false }),
}))
