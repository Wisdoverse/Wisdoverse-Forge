import { describe, it, expect } from 'vitest'
import {
  chatStreamEventErrorMessage,
  chatStreamHttpErrorMessage,
  chatStreamRequestErrorMessage,
  parseSseFrame,
} from '@app/features/chat/useChatStream'

function expectBeginnerMessage(actual: string, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Details:')
}

describe('parseSseFrame', () => {
  it('parses message_start frame with JSON data', () => {
    const frame = parseSseFrame(
      'event: message_start\ndata: {"message_id":"abc","model":"claude-sonnet-4-6"}\n\n'
    )
    expect(frame).toEqual({
      event: 'message_start',
      data: { message_id: 'abc', model: 'claude-sonnet-4-6' },
    })
  })

  it('returns null for incomplete frame (no trailing \\n\\n)', () => {
    expect(parseSseFrame('event: delta\n')).toBeNull()
  })

  it('handles delta frames', () => {
    const frame = parseSseFrame('event: delta\ndata: {"text":"Hello"}\n\n')
    expect(frame).toEqual({ event: 'delta', data: { text: 'Hello' } })
  })

  it('handles message_stop with tokens + finish_reason', () => {
    const frame = parseSseFrame(
      'event: message_stop\ndata: {"tokens_in":12,"tokens_out":34,"finish_reason":"stop"}\n\n'
    )
    expect(frame).toEqual({
      event: 'message_stop',
      data: { tokens_in: 12, tokens_out: 34, finish_reason: 'stop' },
    })
  })

  it('handles error frames', () => {
    const frame = parseSseFrame(
      'event: error\ndata: {"code":"provider_error","message":"rate limited","retryable":true}\n\n'
    )
    expect(frame).toEqual({
      event: 'error',
      data: { code: 'provider_error', message: 'rate limited', retryable: true },
    })
  })

  it('falls back to string data when JSON parse fails', () => {
    const frame = parseSseFrame('event: delta\ndata: not-json\n\n')
    expect(frame).toEqual({ event: 'delta', data: 'not-json' })
  })
})

describe('chatStreamHttpErrorMessage', () => {
  it('turns auth failures into a clear next step', () => {
    expectBeginnerMessage(
      chatStreamHttpErrorMessage(401),
      'Your sign-in expired. Sign in again, then open this agent chat and resend the message.'
    )
  })

  it('explains missing agent access without exposing raw transport text', () => {
    const message = chatStreamHttpErrorMessage(404, { message: 'agent missing' })

    expectBeginnerMessage(
      message,
      'This agent could not be found. Refresh the Agents page, choose an active agent, then open chat again.'
    )
    expect(message).not.toContain('agent missing')
  })

  it('turns busy agent conflicts into a wait step', () => {
    expectBeginnerMessage(
      chatStreamHttpErrorMessage(409, { message: 'agent is busy' }),
      'This agent is already working. Wait for the current reply to finish, then resend the message.'
    )
  })

  it('turns chat rate limits into an operator action', () => {
    const message = chatStreamHttpErrorMessage(429)

    expectBeginnerMessage(
      message,
      'This agent is receiving too many messages right now. Wait a moment, then resend the message.'
    )
    expect(message).not.toContain('provider')
    expect(message).not.toContain('model service')
  })

  it('keeps server failures actionable for first-time operators', () => {
    const message = chatStreamHttpErrorMessage(503, { error: 'service unavailable' })

    expectBeginnerMessage(
      message,
      'Forge could not send this chat message right now. Wait a few minutes, then resend it. If it still fails, ask an owner or admin to check chat and agent setup.'
    )
    expect(message).not.toContain('service unavailable')
  })

  it('keeps fallback send errors about the message, not transport details', () => {
    const message = chatStreamHttpErrorMessage(418, { error: 'teapot route' })

    expectBeginnerMessage(
      message,
      'This message was not sent. Refresh this agent, then resend the message.'
    )
    expect(message).not.toContain('chat request')
    expect(message).not.toContain('teapot route')
  })
})

describe('chatStreamEventErrorMessage', () => {
  it('maps streamed rate limits without exposing raw event details', () => {
    const message = chatStreamEventErrorMessage('provider_error: rate limited')

    expectBeginnerMessage(
      message,
      'This agent is receiving too many messages right now. Wait a moment, then resend the message.'
    )
    expect(message).not.toContain('provider_error')
  })

  it('maps streamed permission failures to role guidance', () => {
    const message = chatStreamEventErrorMessage('Forbidden token scope')

    expectBeginnerMessage(
      message,
      'You do not have access to this agent chat. Ask an owner or admin to update your workspace role.'
    )
    expect(message).not.toContain('token')
  })

  it('maps context limit errors to old-message guidance', () => {
    const message = chatStreamEventErrorMessage('context window exceeded')

    expectBeginnerMessage(
      message,
      'This chat has too many old messages. Clear chat only if those messages are no longer useful, then send the message again.'
    )
    expect(message).not.toContain('old context')
    expect(message).not.toContain('context window')
  })

  it('maps unknown streamed failures to a resend and owner check step', () => {
    const message = chatStreamEventErrorMessage('stream error')

    expectBeginnerMessage(
      message,
      'The agent could not finish this reply. Resend the message. If it still fails, ask an owner or admin to check chat setup.'
    )
    expect(message).not.toContain('stream error')
  })
})

describe('chatStreamRequestErrorMessage', () => {
  it('starts network failures with the resend step', () => {
    expectBeginnerMessage(
      chatStreamRequestErrorMessage(new TypeError('Failed to fetch')),
      'Check your connection, then resend the message. Forge could not connect while sending this message.'
    )
  })

  it('keeps user-canceled sends quiet', () => {
    const error = new DOMException('The user aborted a request.', 'AbortError')

    expect(chatStreamRequestErrorMessage(error)).toBe('')
  })
})
