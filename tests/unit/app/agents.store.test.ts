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
      'Sign in again, then open Agents again to load agents.'
    )
  })

  test('turns permission failures into team space access guidance', () => {
    const message = agentActionErrorMessage(
      'delete',
      apiError(403, { message: 'owner role required' })
    )

    expectBeginnerError(
      message,
      'Ask an owner or admin to update your team space access, then choose Delete again. You do not have permission to delete the agent.'
    )
    expect(message).not.toContain('workspace role')
    expect(message).not.toContain('try to delete')
  })

  test('turns plain role failures into team space access guidance', () => {
    const message = agentActionErrorMessage('delete', 'owner role required')

    expectBeginnerError(
      message,
      'Ask an owner or admin to update your team space access, then choose Delete again. You do not have permission to delete the agent.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns raw network failures into connection guidance', () => {
    const message = agentActionErrorMessage('start', 'Network error')

    expectBeginnerError(
      message,
      'Check your connection, then open Agents and choose this agent again. Forge could not start the agent while updating Agents.'
    )
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('service')
  })

  test('turns agent load network failures into an Agents page step', () => {
    const message = agentActionErrorMessage('load', 'Network error')

    expectBeginnerError(message, 'Check your connection, then open Agents again to load agents.')
    expect(message).not.toContain('Network error')
    expect(message).not.toContain('service')
  })

  test('keeps create network failures on the New agent path', () => {
    const message = agentActionErrorMessage('create', 'Network error')

    expectBeginnerError(
      message,
      'Check your connection, then open Agents and choose New agent again. Forge could not create the agent.'
    )
    expect(message).not.toContain('choose this agent')
    expect(message).not.toContain('Check the agent details')
    expect(message).not.toContain('refresh')
  })

  test('turns status fields into a wait and retry step', () => {
    expectBeginnerError(
      agentActionErrorMessage('sendPrompt', {
        ok: false,
        status: '429',
        error: 'rate limit exceeded',
      }),
      'Wait a moment, then send the instruction again. The Agents page is busy.'
    )
  })

  test('uses AI service language for create validation failures', () => {
    expectBeginnerError(
      agentActionErrorMessage('create', apiError(422, { message: 'provider and model required' })),
      'Choose a tested AI service and model, then choose Add agent again.'
    )
  })

  test('keeps unclear create validation on the New agent form path', () => {
    const message = agentActionErrorMessage(
      'create',
      apiError(422, { message: 'payload was not accepted' })
    )

    expectBeginnerError(
      message,
      'Check the name, where this agent should work, and any required service or project, then choose Add agent again.'
    )
    expect(message).not.toContain('Check the agent details')
    expect(message).not.toContain('choose this agent')
    expect(message).not.toContain('refresh')
  })

  test('keeps internal create failures on the service recovery path', () => {
    const message = agentActionErrorMessage('create', {
      ok: false,
      error: { code: 'INTERNAL_ERROR', message: 'Internal server error' },
    })

    expectBeginnerError(
      message,
      'Wait a moment, then open Agents and choose New agent again. Forge could not prepare project files for agents right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(message).not.toContain('Check the agent details')
    expect(message).not.toContain('file work')
  })

  test('explains managed workspace startup failures without worker jargon', () => {
    const message = agentActionErrorMessage(
      'start',
      apiError(422, { message: 'Docker daemon unavailable' })
    )

    expectBeginnerError(
      message,
      'Ask an owner or admin to check Where agents work in Settings, then start this agent from the agent card. Project files are not ready.'
    )
    expect(message).not.toContain('worker')
    expect(message).not.toContain('Docker')
    expect(message).not.toContain('workspace is not ready')
    expect(message).not.toContain('place where this agent runs')
    expect(message).not.toContain('File work')
  })

  test('starts unknown action failures with an Agents page step', () => {
    const message = agentActionErrorMessage('restart', apiError(418, { message: 'teapot' }))

    expectBeginnerError(
      message,
      'Open Agents, choose this agent, then restart it again. Forge could not restart the agent.'
    )
    expect(message).not.toContain('teapot')
    expect(message).not.toContain('try to restart')
  })

  test('keeps missing load failures on the Agents page', () => {
    const message = agentActionErrorMessage('load', apiError(404, { message: 'not found' }))

    expectBeginnerError(message, 'Open Agents again to load agents. This agent could not be found.')
    expect(message).not.toContain('choose this agent')
  })

  test('keeps missing create targets on the New agent path', () => {
    const message = agentActionErrorMessage('create', apiError(404, { message: 'not found' }))

    expectBeginnerError(
      message,
      'Open Agents, choose the current project, then choose New agent again. The project or team space changed while creating this agent.'
    )
    expect(message).not.toContain('This agent could not be found')
    expect(message).not.toContain('Check the agent details')
    expect(message).not.toContain('refresh')
  })

  test('keeps missing this-computer setup targets on the New agent path', () => {
    const message = agentActionErrorMessage('enrollLocal', apiError(404, { message: 'not found' }))

    expectBeginnerError(
      message,
      'Open Agents, choose the current project, then choose New agent again. The project or team space changed before this computer setup was ready.'
    )
    expect(message).not.toContain('This agent could not be found')
    expect(message).not.toContain('Check the agent details')
    expect(message).not.toContain('refresh')
  })

  test('starts delete conflicts with a current-status check step', () => {
    expectBeginnerError(
      agentActionErrorMessage('delete', apiError(409, { message: 'version changed' })),
      'Open Agents, check the current status, then choose Delete again. This agent changed while you were deleting it.'
    )
  })

  test('uses team space language for create access validation', () => {
    const message = agentActionErrorMessage(
      'create',
      apiError(422, { message: 'workspace project access is required' })
    )

    expectBeginnerError(
      message,
      'Choose a team space and project you can access, then choose Add agent again.'
    )
    expect(message).not.toContain('Choose a workspace')
    expect(message).not.toContain('try creating')
  })

  test('explains this-computer agent validation without CLI jargon', () => {
    const message = agentActionErrorMessage(
      'enrollLocal',
      apiError(422, { message: 'cli tool is required' })
    )

    expectBeginnerError(
      message,
      'Check the agent name, agent type, and project, then choose New agent again.'
    )
    expect(message).not.toContain('CLI')
    expect(message).not.toContain('work tool')
    expect(message).not.toContain('project access')
    expect(message).not.toContain('local agent')
  })

  test('explains why this-computer setup still needs a project', () => {
    const message = agentActionErrorMessage(
      'enrollLocal',
      apiError(422, { message: 'project_id is required' })
    )

    expectBeginnerError(
      message,
      'Choose a project you can access, then choose Add agent again. This computer agents still need a project so Tasks and history have a place to save.'
    )
    expect(message).not.toContain('project_id')
    expect(message).not.toContain('workspace')
    expect(message).not.toContain('local agent')
  })

  test('explains this-computer enrollment server failures before setup commands exist', () => {
    const message = agentActionErrorMessage(
      'enrollLocal',
      apiError(503, { message: 'database unavailable' })
    )

    expectBeginnerError(
      message,
      'Wait a moment, then open Agents and choose New agent again. Forge could not prepare the setup text for this computer right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(message).not.toContain('database')
    expect(message).not.toContain('local agent')
    expect(message).not.toContain('setup command')
  })

  test('turns project-file server failures into concrete agent action steps', () => {
    expectBeginnerError(
      agentActionErrorMessage('create', apiError(503, { message: 'runtime unavailable' })),
      'Wait a moment, then open Agents and choose New agent again. Forge could not prepare project files for agents right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expectBeginnerError(
      agentActionErrorMessage('start', apiError(503, { message: 'runtime unavailable' })),
      'Wait a moment, then open Agents, choose this agent, and start it again. Forge could not prepare project files for agents right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expectBeginnerError(
      agentActionErrorMessage('restart', apiError(503, { message: 'runtime unavailable' })),
      'Wait a moment, then open Agents, choose this agent, and restart it again. Forge could not prepare project files for agents right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
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

    expectBeginnerError(
      useAgentsStore.getState().error,
      'Open Agents again to load agents. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(useAgentsStore.getState().error).not.toContain('temporarily unavailable')
    expect(useAgentsStore.getState().loading).toBe(false)
  })

  test('stores an Agents page step when agent loading returns unclear details', async () => {
    agentApiMock.getAgents.mockResolvedValue({
      ok: false,
      details: { reason: 'agent list parser detail' },
    })

    await useAgentsStore.getState().loadAgents()

    expectBeginnerError(useAgentsStore.getState().error, 'Open Agents again to load agents.')
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
      'Name this agent, choose where it should work, then choose Add agent again.'
    )
    expect(useAgentsStore.getState().error).not.toContain('try creating')
  })

  test('stores the requested create-agent starting choice while opening the modal', () => {
    useAgentsStore.getState().setCreateModalOpen(true, 'local-cli')

    expect(useAgentsStore.getState().createModalOpen).toBe(true)
    expect(useAgentsStore.getState().createModalInitialKind).toBe('local-cli')

    useAgentsStore.getState().setCreateModalOpen(false)

    expect(useAgentsStore.getState().createModalOpen).toBe(false)
    expect(useAgentsStore.getState().createModalInitialKind).toBeNull()
  })

  test('stores a check step when agent instructions are not saved', async () => {
    agentApiMock.updateAgent.mockResolvedValue({
      ok: false,
      error: 'database unavailable',
    })

    const result = await useAgentsStore
      .getState()
      .updateAgentSystemPrompt('agent-1', 'Explain risks plainly.')

    expect(result).toBe(false)
    expectBeginnerError(
      useAgentsStore.getState().error,
      'Check the instruction text, open this agent again, then save the instructions again.'
    )
    expect(useAgentsStore.getState().error).not.toContain('Review the instructions')
    expect(useAgentsStore.getState().error).not.toContain('Agent instructions were not saved')
    expect(useAgentsStore.getState().error).not.toContain('database')
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
      'Ask an owner or admin to check Where agents work in Settings, then start this agent from the card. Agent was created, but project files are not ready yet. It will stay in the list.'
    )
    expect(state.error).not.toContain('worker')
    expect(state.error).not.toContain('Docker')
    expect(state.error).not.toContain('workspace is not ready')
    expect(state.error).not.toContain('the place where it runs')
    expect(state.error).not.toContain('file work')
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
      'Check your connection, then choose New agent again. Forge could not prepare the setup text for this computer.'
    )
    expect(useAgentsStore.getState().error).not.toContain('Network error')
    expect(useAgentsStore.getState().error).not.toContain('local agent')
    expect(useAgentsStore.getState().error).not.toContain('setup command')
  })

  test('stores setup text retry guidance when this-computer enrollment is unavailable', async () => {
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
      'Wait a moment, then open Agents and choose New agent again. Forge could not prepare the setup text for this computer right now. If it still fails, ask an owner or admin to check Where agents work in Settings.'
    )
    expect(useAgentsStore.getState().error).not.toContain('database')
    expect(useAgentsStore.getState().error).not.toContain('local agent')
    expect(useAgentsStore.getState().error).not.toContain('setup command')
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
      'Wait for the current work to finish, open Agents and choose this agent again, then send the instruction again. This agent is already working.'
    )
  })
})
