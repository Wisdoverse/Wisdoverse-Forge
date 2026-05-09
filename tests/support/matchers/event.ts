import { expect } from 'vitest'

expect.extend({
  toMatchEventShape(received: Record<string, unknown>, expectedType: string) {
    const hasId = typeof received.id === 'string'
    const hasTimestamp = typeof received.timestamp === 'number'
    const hasType = received.type === expectedType
    const hasSessionId = typeof received.sessionId === 'string'
    const pass = hasId && hasTimestamp && hasType && hasSessionId
    const missing: string[] = []
    if (!hasId) missing.push('id (string)')
    if (!hasTimestamp) missing.push('timestamp (number)')
    if (!hasType) missing.push(`type === "${expectedType}" (got "${received.type}")`)
    if (!hasSessionId) missing.push('sessionId (string)')
    return {
      pass,
      message: () =>
        pass
          ? `expected event NOT to match shape for "${expectedType}"`
          : `event missing required fields: ${missing.join(', ')}`,
    }
  },
})

declare module 'vitest' {
  interface Assertion<T> {
    toMatchEventShape(eventType: string): T
  }
  interface AsymmetricMatchersContaining {
    toMatchEventShape(eventType: string): unknown
  }
}
