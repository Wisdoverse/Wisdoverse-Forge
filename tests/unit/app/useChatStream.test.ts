import { describe, it, expect } from 'vitest'
import { chatStreamHttpErrorMessage, parseSseFrame } from '@app/features/chat/useChatStream'

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
      'Sign in again, then open the agent chat and resend the message.'
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

  it('turns provider rate limits into an operator action', () => {
    expectBeginnerMessage(
      chatStreamHttpErrorMessage(429),
      'The provider is limiting messages right now. Wait a moment, then resend the message.'
    )
  })

  it('keeps server failures actionable for first-time operators', () => {
    const message = chatStreamHttpErrorMessage(503, { error: 'service unavailable' })

    expectBeginnerMessage(
      message,
      'The chat service is temporarily unavailable. Ask an owner or admin to check the backend and agent runtime, then resend the message.'
    )
    expect(message).not.toContain('service unavailable')
  })
})
