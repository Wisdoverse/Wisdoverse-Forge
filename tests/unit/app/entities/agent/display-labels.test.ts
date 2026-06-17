import { describe, expect, it } from 'vitest'

import { agentAiServiceLabel, agentRuntimeLabel } from '@app/entities/agent'

describe('agent display labels', () => {
  it('turns missing AI service values into a refresh step', () => {
    expect(agentAiServiceLabel(null)).toBe('Refresh AI service')
    expect(agentAiServiceLabel(' ')).toBe('Refresh AI service')
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
