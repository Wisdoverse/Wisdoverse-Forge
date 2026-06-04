import { describe, test, expect, beforeEach, vi } from 'vitest'

const agentApiMock = vi.hoisted(() => ({
  getAgents: vi.fn(),
  createAgent: vi.fn(),
  enrollLocalAgent: vi.fn(),
  updateAgent: vi.fn(),
  deleteAgent: vi.fn(),
  sendPrompt: vi.fn(),
  startAgent: vi.fn(),
  restartAgent: vi.fn(),
}))

vi.mock('@app/shared/api/legacy', () => ({
  getAgentApi: () => agentApiMock,
}))

import { agentActionErrorMessage, useAgentsStore } from '@app/entities/agent'

function apiError(status: number, payload: Record<string, unknown> | string): Error {
  const body = typeof payload === 'string' ? payload : JSON.stringify(payload)
  return new Error(`API ${status}: ${body}`)
}

function managedAgent(overrides: Record<string, unknown> = {}) {
  return {
    id: 'agent-1',
    name: 'Review Agent',
    status: 'idle',
    provider: 'openai',
    model: 'gpt-4o',
    cliTool: null,
    runtimeId: null,
    cwd: null,
    containerId: null,
    workspaceId: null,
    workspaceName: null,
    projectId: null,
    projectName: null,
    systemPrompt: null,
    ...overrides,
  }
}

beforeEach(() => {
  useAgentsStore.getState().reset()
  Object.values(agentApiMock).forEach((mock) => mock.mockReset())
})

describe('Agents Store', () => {
  test('turns expired sessions into a sign-in step', () => {
    expect(agentActionErrorMessage('load', apiError(401, { error: 'token expired' }))).toBe(
      'Sign in again, then load agents. Code: 401. Details: token expired'
    )
  })

  test('turns permission failures into workspace role guidance', () => {
    expect(
      agentActionErrorMessage('delete', apiError(403, { message: 'owner role required' }))
    ).toBe(
      'You do not have permission to delete the agent. Ask an admin to update your workspace role. Code: 403. Details: owner role required'
    )
  })

  test('turns raw network failures into connection guidance', () => {
    expect(agentActionErrorMessage('start', 'Network error')).toBe(
      'Agent setup could not start the agent because the browser could not reach the server. Check your connection and refresh the page.'
    )
  })

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

  test('stores beginner guidance when agent loading has a server failure', async () => {
    agentApiMock.getAgents.mockRejectedValue(
      apiError(503, { error: { message: 'database unavailable' } })
    )

    await useAgentsStore.getState().loadAgents()

    expect(useAgentsStore.getState().error).toBe(
      'The agent service had a server problem. Try again after the backend is healthy. Code: 503. Details: database unavailable'
    )
    expect(useAgentsStore.getState().loading).toBe(false)
  })

  test('stores field guidance when agent creation is invalid', async () => {
    agentApiMock.createAgent.mockResolvedValue({
      ok: false,
      details: { reason: 'name is required' },
    })

    const result = await useAgentsStore.getState().createAgent({
      name: '',
      kind: 'provider',
      provider: 'openai',
      model: 'gpt-4o',
    })

    expect(result).toBe(false)
    expect(useAgentsStore.getState().error).toBe(
      'Agent setup could not create the agent. Review the message and try again. Details: name is required'
    )
  })

  test('keeps created agent visible when container start needs operator action', async () => {
    agentApiMock.createAgent.mockResolvedValue({
      ok: true,
      agent: managedAgent({ id: 'cli-1', cliTool: 'claude', model: null, provider: null }),
    })
    agentApiMock.startAgent.mockResolvedValue({
      ok: false,
      error: 'Docker is not running',
    })

    const result = await useAgentsStore.getState().createAgent({
      name: 'CLI Agent',
      kind: 'cli',
      cliTool: 'claude',
    })

    expect(result).toBe(true)
    expect(useAgentsStore.getState().agents).toHaveLength(1)
    expect(useAgentsStore.getState().error).toBe(
      'Agent was created, but it could not start yet. It will stay in the list. Check the agent runtime, then start it from the agent card. Details: Docker is not running'
    )
  })

  test('stores connection guidance when local agent enrollment cannot reach the server', async () => {
    agentApiMock.enrollLocalAgent.mockResolvedValue({ ok: false, error: 'Network error' })

    const result = await useAgentsStore.getState().enrollLocalAgent({
      name: 'Local Agent',
      cliTool: 'codex',
    })

    expect(result).toBeNull()
    expect(useAgentsStore.getState().error).toBe(
      'Agent setup could not connect the local agent because the browser could not reach the server. Check your connection and refresh the page.'
    )
  })

  test('stores retry guidance when prompt send hits a conflict', async () => {
    agentApiMock.sendPrompt.mockResolvedValue({
      ok: false,
      error: 'agent is already working',
      statusCode: 409,
    })

    const result = await useAgentsStore.getState().sendPrompt('agent-1', 'Start the task')

    expect(result).toBe(false)
    expect(useAgentsStore.getState().error).toBe(
      'This agent changed while you were working. Refresh the Agents page, review its current status, then try again. Code: 409. Details: agent is already working'
    )
  })
})
