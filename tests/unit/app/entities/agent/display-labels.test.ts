import { describe, expect, it } from 'vitest'

import { agentAiServiceLabel, agentRuntimeLabel } from '@app/entities/agent'

describe('agent display labels', () => {
  it('turns missing AI service values into a setup check', () => {
    expect(agentAiServiceLabel(null)).toBe('Check AI service setup')
    expect(agentAiServiceLabel(' ')).toBe('Check AI service setup')
  })

  it('keeps known and check-needed AI service values readable', () => {
    expect(agentAiServiceLabel('openai')).toBe('OpenAI AI service')
    expect(agentAiServiceLabel('future_provider')).toBe('Check AI service')
  })

  it('labels file-capable agents by what users can do with them', () => {
    expect(
      agentRuntimeLabel({
        id: 'agent-1',
        name: 'Build Agent',
        provider: null,
        model: null,
        status: 'idle',
        tasksCompleted: 0,
        tasksInProgress: 0,
        successRate: 0,
        cliTool: 'opencode',
      })
    ).toBe('OpenCode with project files')
  })
})
