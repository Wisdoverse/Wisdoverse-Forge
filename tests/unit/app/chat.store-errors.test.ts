import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { chatErrorMessage } from '@app/shared/model/chat.errors'
import { useChatStore } from '@app/shared/model/chat.store'

const agentApiMock = vi.hoisted(() => ({
  fetchMessages: vi.fn(),
  deleteMessages: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getAgentApi: () => agentApiMock,
}))

function resetChatState() {
  useChatStore.setState({
    turns: [],
    loading: false,
    error: null,
    messages: [],
    streaming: false,
    streamingMessageId: null,
    messagesLoading: false,
  })
}

describe('chatErrorMessage', () => {
  test('maps load permission errors to agent access guidance', () => {
    expect(chatErrorMessage('load', new Error('HTTP 403'))).toBe(
      'Retry conversation to load conversation history. Ask an owner or admin to give you access to this agent.'
    )
  })

  test('maps structured sign-in errors without exposing token details', () => {
    const message = chatErrorMessage('load', {
      code: '401',
      detail: 'unauthorized chat token expired',
    })

    expect(message).toBe(
      'Retry conversation to load conversation history. Sign in again, then reopen this chat.'
    )
    expect(message).not.toContain('chat token expired')
  })

  test('uses server error details for structured sign-in failures', () => {
    const message = chatErrorMessage('load', {
      serverError: 'unauthorized chat session expired',
      statusCode: '401',
    })

    expect(message).toBe(
      'Retry conversation to load conversation history. Sign in again, then reopen this chat.'
    )
    expect(message).not.toContain('chat session expired')
  })

  test('maps structured clear conflicts to a wait and retry step', () => {
    const message = chatErrorMessage('clear', {
      reason: 'conversation delete already in progress',
      statusCode: 409,
    })

    expect(message).toBe(
      'Chat was not cleared. Another chat action is still saving. Wait a moment, then try again.'
    )
    expect(message).not.toContain('delete already in progress')
  })

  test('maps structured rate limits without raw provider text', () => {
    const message = chatErrorMessage('load', {
      error: 'too many provider history reads',
      status: 429,
    })

    expect(message).toBe(
      'Retry conversation to load conversation history. Too many chat requests are happening right now. Wait a minute, then try again.'
    )
    expect(message).not.toContain('provider history')
  })

  test('maps server errors without exposing transport text', () => {
    const message = chatErrorMessage('clear', 'Server error (503)')

    expect(message).toBe(
      'Chat was not cleared. Forge could not update this chat right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check chat setup.'
    )
    expect(message).not.toContain('503')
    expect(message).not.toContain('platform')
  })

  test('maps unusable conversation data without exposing raw response wording', () => {
    const message = chatErrorMessage('load', new Error('Server returned ok: false'))

    expect(message).toBe(
      'Retry conversation to load conversation history. Forge could not read this conversation. Refresh the chat, then try again.'
    )
    expect(message).not.toContain('ok: false')
  })
})

describe('useChatStore beginner errors', () => {
  beforeEach(() => {
    resetChatState()
    agentApiMock.fetchMessages.mockReset()
    agentApiMock.deleteMessages.mockReset()
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  test('turns failed message loads into retryable guidance', async () => {
    agentApiMock.fetchMessages.mockResolvedValue({ ok: false, error: 'Server error (503)' })

    await useChatStore.getState().loadMessages('agent-1')

    expect(useChatStore.getState().error).toBe(
      'Retry conversation to load conversation history. Forge could not load this conversation right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check chat setup.'
    )
    expect(useChatStore.getState().error).not.toContain('503')
    expect(useChatStore.getState().error).not.toContain('platform')
    expect(useChatStore.getState().messagesLoading).toBe(false)
  })

  test('turns message load network exceptions into connection guidance', async () => {
    agentApiMock.fetchMessages.mockRejectedValue(new TypeError('Failed to fetch'))

    await useChatStore.getState().loadMessages('agent-1')

    expect(useChatStore.getState().error).toBe(
      'Retry conversation to load conversation history. Check your connection, then choose Retry conversation again. Forge could not connect while loading this conversation.'
    )
    expect(useChatStore.getState().error).not.toContain('Failed to fetch')
  })

  test('turns clear chat failures into access guidance', async () => {
    agentApiMock.deleteMessages.mockRejectedValue(new Error('HTTP 403'))

    await useChatStore.getState().clearMessages('agent-1')

    expect(useChatStore.getState().error).toBe(
      'Chat was not cleared. Ask an owner or admin to give you access to this agent.'
    )
    expect(useChatStore.getState().error).not.toContain('HTTP 403')
  })

  test('turns event history failures into the same conversation recovery copy', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
      })
    )

    await useChatStore.getState().fetchEvents('agent-1')

    expect(useChatStore.getState().error).toBe(
      'Retry conversation to load conversation history. Forge could not load this conversation right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check chat setup.'
    )
    expect(useChatStore.getState().error).not.toContain('HTTP 500')
    expect(useChatStore.getState().loading).toBe(false)
  })
})
