import { describe, expect, it } from 'vitest'

import { extractApiError } from '../../../src/app/shared/api/agent-api-types'

describe('extractApiError', () => {
  it('uses an actionable beginner fallback when the response has no message', () => {
    expect(extractApiError({})).toBe('Refresh, then try again. Forge did not return a clear error.')
  })
})
