import { describe, expect, it } from 'vitest'

import { agentStatusKey, agentStatusLabel } from '@app/entities/agent'

describe('agent status labels', () => {
  it('uses beginner-safe labels for unknown statuses', () => {
    expect(agentStatusLabel(null)).toBe('Check if ready')
    expect(agentStatusLabel('future_status')).toBe('Check if ready')
    expect(agentStatusLabel('future_status')).not.toBe('Check agent status')
  })

  it('normalizes machine status values before showing labels', () => {
    expect(agentStatusKey(' Idle ')).toBe('idle')
    expect(agentStatusLabel(' Idle ')).toBe('Ready')
    expect(agentStatusLabel('offline')).toBe('Not connected')
  })
})
