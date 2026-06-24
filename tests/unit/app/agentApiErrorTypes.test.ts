import { describe, expect, it } from 'vitest'

import { extractApiError } from '../../../src/app/shared/api/agent-api-types'

describe('extractApiError', () => {
  it('uses an actionable beginner fallback when the response has no message', () => {
    const message = extractApiError({})

    expect(message).toBe(
      'Open this page again, then try again. Forge did not return a clear error. If it still fails, ask an owner or admin to check the service connection.'
    )
    expect(message).not.toContain('Refresh')
  })
})
