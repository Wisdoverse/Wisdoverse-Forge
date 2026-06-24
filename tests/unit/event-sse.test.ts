import { describe, expect, it } from 'vitest'

import {
  encodeCursor,
  decodeCursor,
} from '../../shared/types/stream-events.js'

describe('SSE cursor encoding', () => {
  it('round-trips cursor encode/decode', () => {
    const original = { ts: 1711100000000, id: 'evt-uuid-456' }
    const encoded = encodeCursor(original)
    const decoded = decodeCursor(encoded)

    expect(decoded).toEqual(original)
  })

  it('produces an opaque base64url string', () => {
    const encoded = encodeCursor({ ts: 1711100000000, id: 'evt-1' })
    // Should not contain raw timestamp or event ID
    expect(encoded).not.toContain('1711100000000')
    expect(encoded).not.toContain('evt-1')
    // Should be valid base64url (no +, /, =)
    expect(encoded).toMatch(/^[A-Za-z0-9_-]+$/)
  })

  it('returns null for malformed cursors', () => {
    expect(decodeCursor('')).toBeNull()
    expect(decodeCursor('not-valid')).toBeNull()
    expect(decodeCursor('abc123')).toBeNull()
  })

  it('returns null for cursor missing id', () => {
    // Encode just a timestamp without colon separator
    const bad = Buffer.from('12345').toString('base64url')
    expect(decodeCursor(bad)).toBeNull()
  })

  it('returns null for cursor with non-numeric timestamp', () => {
    const bad = Buffer.from('notanumber:evt-1').toString('base64url')
    expect(decodeCursor(bad)).toBeNull()
  })
})
