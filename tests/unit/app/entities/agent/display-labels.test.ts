import { describe, expect, it } from 'vitest'

import { agentAiServiceLabel } from '@app/entities/agent'

describe('agent display labels', () => {
  it('turns missing AI service values into a refresh step', () => {
    expect(agentAiServiceLabel(null)).toBe('Refresh AI service')
    expect(agentAiServiceLabel(' ')).toBe('Refresh AI service')
  })

  it('keeps known and check-needed AI service values readable', () => {
    expect(agentAiServiceLabel('openai')).toBe('OpenAI AI service')
    expect(agentAiServiceLabel('future_provider')).toBe('Check AI service')
  })
})
