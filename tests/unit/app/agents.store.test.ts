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

  test('turns permission failures into team space access guidance', () => {
    const message = agentActionErrorMessage('delete', apiError(403, { message: 'owner role required' }))

    expectBeginnerError(
      message,
      'You do not have permission to delete the agent. Ask an owner or admin to update your team space access.'
    )
    expect(message).not.toContain('workspace role')
  })

  test('turns raw network failures into connection guidance', () => {
    const message = agentActionErrorMessage('start', 'Network error')

    expectBeginnerError(
      message,
      'Forge could not start the agent. It could not connect while updating Agents. Check your connection, then refresh Agents.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('service')
  })

  test('turns agent load network failures into a refresh step', () => {
    const message = agentActionErrorMessage('load', 'Network error')

    expectBeginnerError(message, 'Check your connection, then refresh Agents to load agents.')
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('service')
  })

  test('turns status fields into a wait and retry step', () => {
    expectBeginnerError(
      agentActionErrorMessage('sendPrompt', {
        ok: false,
        status: '429',
        error: 'rate limit exceeded',
      }),
      'The Agents page is busy. Wait a moment, then try to send the instruction again.'
    )
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
      'The place where this agent runs is not ready. Ask an owner or admin to check Where agents run, then start this agent from the agent card.'
    )
    expect(message).not.toContain('worker')
    expect(message).not.toContain('Docker')
    expect(message).not.toContain('workspace is not ready')
  })

  test('uses team space language for create access validation', () => {
    const message = agentActionErrorMessage(
      'create',
      apiError(422, { message: 'workspace project access is required' })
    )

    expectBeginnerError(
      message,
      'Choose a team space and project you can access, then try creating this agent again.'
    )
    expect(message).not.toContain('Choose a workspace')
  })

  test('explains this-computer agent validation without CLI jargon', () => {
    const message = agentActionErrorMessage(
      'enrollLocal',
      apiError(422, { message: 'cli tool is required' })
    )

    expectBeginnerError(
      message,
      'Check the agent name, work tool, and project access, then choose Create Agent again.'
    )
    expect(message).not.toContain('CLI')
    expect(message).not.toContain('local agent')
  })

  test('explains this-computer enrollment server failures before setup commands exist', () => {
    const message = agentActionErrorMessage(
      'enrollLocal',
      apiError(503, { message: 'database unavailable' })
    )

    expectBeginnerError(
      message,
      'Forge could not prepare the setup command for this computer right now. Wait a moment, then choose Create Agent again. If it still fails, ask an owner or admin to check Where agents run.'
    )
    expect(message).not.toContain('database')
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
      'Refresh Agents to load agents. If it still fails, ask an owner or admin to check Where agents run.'
    )
    expect(useAgentsStore.getState().error).not.toContain('temporarily unavailable')
    expect(useAgentsStore.getState().loading).toBe(false)
  })

  test('stores a refresh step when agent loading returns unclear details', async () => {
    agentApiMock.getAgents.mockResolvedValue({
      ok: false,
      details: { reason: 'agent list parser detail' },
    })

    await useAgentsStore.getState().loadAgents()

    expectBeginnerError(useAgentsStore.getState().error, 'Refresh Agents to load agents.')
    expect(useAgentsStore.getState().error).not.toContain('parser')
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

  test('stores the requested create-agent starting choice while opening the modal', () => {
    useAgentsStore.getState().setCreateModalOpen(true, 'local-cli')

    expect(useAgentsStore.getState().createModalOpen).toBe(true)
    expect(useAgentsStore.getState().createModalInitialKind).toBe('local-cli')

    useAgentsStore.getState().setCreateModalOpen(false)

    expect(useAgentsStore.getState().createModalOpen).toBe(false)
    expect(useAgentsStore.getState().createModalInitialKind).toBeNull()
  })

  test('keeps created agent and modal visible when container start needs operator action', async () => {
    useAgentsStore.setState({ createModalOpen: true })
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

    const state = useAgentsStore.getState()
    expect(result).toBe(true)
    expect(state.createModalOpen).toBe(true)
    expect(state.agents).toHaveLength(1)
    expectBeginnerError(
      state.error,
      'Agent was created, but the place where it runs is not ready yet. It will stay in the list. Ask an owner or admin to check Where agents run, then start this agent from the card.'
    )
    expect(state.error).not.toContain('worker')
    expect(state.error).not.toContain('Docker')
    expect(state.error).not.toContain('workspace is not ready')
  })

  test('closes the create modal when the managed workspace starts', async () => {
    useAgentsStore.setState({ createModalOpen: true })
    agentApiMock.createAgent.mockResolvedValue({
      ok: true,
      agent: managedAgent({ id: 'cli-1', cliTool: 'claude', model: null, provider: null }),
    })
    agentApiMock.startAgent.mockResolvedValue({ ok: true, containerId: 'container-1' })

    const result = await useAgentsStore.getState().createAgent({
      name: 'CLI Agent',
      kind: 'cli',
      cliTool: 'claude',
    })

    const state = useAgentsStore.getState()
    expect(result).toBe(true)
    expect(state.createModalOpen).toBe(false)
    expect(state.error).toBeNull()
    expect(state.agents[0].containerId).toBe('container-1')
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
      'Forge could not prepare the setup command for this computer. Check your connection, then choose Create Agent again.'
    )
    expect(useAgentsStore.getState().error).not.toContain('Network error')
    expect(useAgentsStore.getState().error).not.toContain('local agent')
  })

  test('stores setup command retry guidance when this-computer enrollment is unavailable', async () => {
    agentApiMock.enrollLocalAgent.mockResolvedValue({
      ok: false,
      error: 'database unavailable',
    })

    const result = await useAgentsStore.getState().enrollLocalAgent({
      name: 'Local Agent',
      cliTool: 'codex',
    })

    expect(result).toBeNull()
    expectBeginnerError(
      useAgentsStore.getState().error,
      'Forge could not prepare the setup command for this computer right now. Wait a moment, then choose Create Agent again. If it still fails, ask an owner or admin to check Where agents run.'
    )
    expect(useAgentsStore.getState().error).not.toContain('database')
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
