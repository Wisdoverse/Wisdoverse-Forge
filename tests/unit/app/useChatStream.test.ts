import { describe, it, expect } from 'vitest'
import { chatStreamHttpErrorMessage, parseSseFrame } from '@app/features/chat/useChatStream'

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
    expect(chatStreamHttpErrorMessage(401)).toBe(
      'Sign in again, then resend the message. The chat request was not authorized. Code: 401.'
    )
  })

  it('explains missing agent access without exposing raw transport text', () => {
    expect(chatStreamHttpErrorMessage(404, { message: 'agent missing' })).toBe(
      'This agent could not be found. Refresh the Agents page and pick an active agent. Code: 404. Details: agent missing'
    )
  })

  it('turns provider rate limits into an operator action', () => {
    expect(chatStreamHttpErrorMessage(429)).toBe(
      'The provider is rate limiting requests. Wait a moment, then resend the message. Code: 429.'
    )
  })

  it('keeps server failures actionable for first-time operators', () => {
    expect(chatStreamHttpErrorMessage(503, { error: 'service unavailable' })).toBe(
      'The chat service had a server problem. Try again after the backend is healthy. Code: 503. Details: service unavailable'
    )
  })
})
