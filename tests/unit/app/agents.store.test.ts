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

function expectBeginnerError(actual: string | null, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Details:')
}

beforeEach(() => {
  useAgentsStore.getState().reset()
  Object.values(agentApiMock).forEach((mock) => mock.mockReset())
})

describe('Agents Store', () => {
  test('turns expired sessions into a sign-in step', () => {
    expectBeginnerError(
      agentActionErrorMessage('load', apiError(401, { error: 'token expired' })),
      'Sign in again, then open Agents and try to load agents again.'
    )
  })

  test('turns permission failures into workspace role guidance', () => {
    expectBeginnerError(
      agentActionErrorMessage('delete', apiError(403, { message: 'owner role required' })),
      'You do not have permission to delete the agent. Ask an owner or admin to update your workspace role.'
    )
  })

  test('turns raw network failures into connection guidance', () => {
    const message = agentActionErrorMessage('start', 'Network error')

    expectBeginnerError(
      message,
      'Agent setup could not start the agent. Forge could not connect while updating Agents. Check your connection, then refresh Agents.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('service')
  })

  test('uses AI service language for create validation failures', () => {
    expectBeginnerError(
      agentActionErrorMessage('create', apiError(422, { message: 'provider and model required' })),
      'Choose a tested AI service and model, then try creating this agent again.'
    )
  })

  test('explains managed workspace startup failures without worker jargon', () => {
    const message = agentActionErrorMessage(
      'start',
      apiError(422, { message: 'Docker daemon unavailable' })
    )

    expectBeginnerError(
      message,
      "This agent's workspace is not ready. Ask an owner or admin to check agent setup, then start this agent from the agent card."
    )
    expect(message).not.toContain('worker')
    expect(message).not.toContain('Docker')
  })

  test('explains this-computer agent validation without CLI jargon', () => {
    const message = agentActionErrorMessage(
      'enrollLocal',
      apiError(422, { message: 'cli tool is required' })
    )

    expectBeginnerError(
      message,
      'Check the agent name, work tool, and project access, then run the join command again.'
    )
    expect(message).not.toContain('CLI')
    expect(message).not.toContain('local agent')
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

    expectBeginnerError(
      useAgentsStore.getState().error,
      'Forge could not update Agents right now. Refresh Agents, then try again. If it still fails, ask an owner or admin to check agent setup.'
    )
    expect(useAgentsStore.getState().error).not.toContain('temporarily unavailable')
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
    expectBeginnerError(
      useAgentsStore.getState().error,
      'Name this agent, choose where it should work, then try creating it again.'
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
    expectBeginnerError(
      useAgentsStore.getState().error,
      'Agent was created, but its workspace is not ready yet. It will stay in the list. Ask an owner or admin to check agent setup, then start this agent from the card.'
    )
    expect(useAgentsStore.getState().error).not.toContain('worker')
    expect(useAgentsStore.getState().error).not.toContain('Docker')
  })

  test('stores connection guidance when this-computer agent enrollment cannot reach the server', async () => {
    agentApiMock.enrollLocalAgent.mockResolvedValue({ ok: false, error: 'Network error' })

    const result = await useAgentsStore.getState().enrollLocalAgent({
      name: 'Local Agent',
      cliTool: 'codex',
    })

    expect(result).toBeNull()
    expectBeginnerError(
      useAgentsStore.getState().error,
      'Agent setup could not connect the agent from this computer. Forge could not connect while updating Agents. Check your connection, then refresh Agents.'
    )
    expect(useAgentsStore.getState().error).not.toContain('Network error')
    expect(useAgentsStore.getState().error).not.toContain('local agent')
  })

  test('stores retry guidance when prompt send hits a conflict', async () => {
    agentApiMock.sendPrompt.mockResolvedValue({
      ok: false,
      error: 'agent is already working',
      statusCode: 409,
    })

    const result = await useAgentsStore.getState().sendPrompt('agent-1', 'Start the task')

    expect(result).toBe(false)
    expectBeginnerError(
      useAgentsStore.getState().error,
      'This agent is already working. Wait for the current work to finish, refresh the Agents page, then try again.'
    )
  })
})
