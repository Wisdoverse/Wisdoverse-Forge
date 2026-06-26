import { describe, it, expect } from 'vitest'
import { consumePromptStream } from '@app/shared/api/promptStream'

/** Build a 200 SSE Response that emits the given raw frames then closes. */
function sseResponse(frames: string[]): Response {
  const encoder = new TextEncoder()
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const frame of frames) controller.enqueue(encoder.encode(frame))
      controller.close()
    },
  })
  return new Response(stream, { status: 200, headers: { 'content-type': 'text/event-stream' } })
}

const START = 'event: message_start\ndata: {"message_id":"m1"}\n\n'
const DELTA = 'event: delta\ndata: {"text":"hi"}\n\n'
const STOP = 'event: message_stop\ndata: {"finish_reason":"stop"}\n\n'

describe('consumePromptStream', () => {
  it('succeeds for a stream that ends with message_stop', async () => {
    const outcome = await consumePromptStream(sseResponse([START, DELTA, STOP]))
    expect(outcome).toEqual({ ok: true })
  })

  it('succeeds on early EOF (no message_stop) — the message was accepted', async () => {
    const outcome = await consumePromptStream(sseResponse([START, DELTA]))
    expect(outcome).toEqual({ ok: true })
  })

  it('succeeds for a 2xx with no body', async () => {
    const outcome = await consumePromptStream(new Response(null, { status: 200 }))
    expect(outcome).toEqual({ ok: true })
  })

  it('fails on a non-2xx before the stream (e.g. the model-vision gate)', async () => {
    const resp = new Response(JSON.stringify({ error: 'model does not support image input' }), {
      status: 400,
      headers: { 'content-type': 'application/json' },
    })
    const outcome = await consumePromptStream(resp)
    expect(outcome.ok).toBe(false)
    expect(outcome.error).toBeTruthy()
  })

  it('maps a 401 to a sign-in recovery message', async () => {
    const outcome = await consumePromptStream(new Response('{}', { status: 401 }))
    expect(outcome.ok).toBe(false)
    expect(outcome.error).toContain('Sign in again')
  })

  it('fails on an in-stream error event', async () => {
    const errorFrame = 'event: error\ndata: {"message":"rate limit exceeded"}\n\n'
    const outcome = await consumePromptStream(sseResponse([START, errorFrame]))
    expect(outcome.ok).toBe(false)
    // rate-limit phrasing flows through chatStreamEventErrorMessage
    expect(outcome.error).toContain('too many messages')
  })

  it('ignores unknown event types (forward-compat) and still succeeds', async () => {
    const unknown = 'event: heartbeat\ndata: {}\n\n'
    const outcome = await consumePromptStream(sseResponse([START, unknown, STOP]))
    expect(outcome).toEqual({ ok: true })
  })

  it('treats a mid-read AbortError as success (composer closed after send)', async () => {
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode(START))
      },
      pull() {
        const err = new Error('aborted')
        err.name = 'AbortError'
        throw err
      },
    })
    const outcome = await consumePromptStream(new Response(stream, { status: 200 }))
    expect(outcome).toEqual({ ok: true })
  })
})
