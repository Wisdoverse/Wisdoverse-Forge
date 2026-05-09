import { describe, test, expect, beforeEach } from 'vitest'
import { useAgentsStore } from '@app/shared/model/agents.store'

beforeEach(() => useAgentsStore.getState().reset())

describe('Agents Store', () => {
  test('initializes with empty agents', () => {
    expect(useAgentsStore.getState().agents).toEqual([])
  })

  test('setAgents replaces agent list', () => {
    useAgentsStore.getState().setAgents([
      {
        id: 'a1',
        name: 'Claude-1',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'online',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      },
      {
        id: 'a2',
        name: 'Gemini-1',
        provider: 'Google',
        model: 'gemini-2.5-pro',
        status: 'offline',
        tasksCompleted: 5,
        tasksInProgress: 0,
        successRate: 0.9,
      },
    ])
    expect(useAgentsStore.getState().agents).toHaveLength(2)
  })

  test('selectAgent sets selectedAgentId', () => {
    useAgentsStore.getState().selectAgent('a1')
    expect(useAgentsStore.getState().selectedAgentId).toBe('a1')
  })

  test('updateAgentStatus updates specific agent', () => {
    useAgentsStore.getState().setAgents([
      {
        id: 'a1',
        name: 'Claude-1',
        provider: 'Anthropic',
        model: 'claude-4-opus',
        status: 'online',
        tasksCompleted: 12,
        tasksInProgress: 1,
        successRate: 0.98,
      },
    ])
    useAgentsStore.getState().updateAgentStatus('a1', 'busy')
    expect(useAgentsStore.getState().agents[0].status).toBe('busy')
  })
})
