import { describe, test, expect, beforeEach, vi } from 'vitest'
import { useAgentsStore } from '@app/entities/agent'
import { getAgentApi } from '@app/shared/api/legacy'

vi.mock('@app/shared/api/legacy', async (importOriginal) => {
  const original = await importOriginal<typeof import('@app/shared/api/legacy')>()
  return { ...original, getAgentApi: vi.fn() }
})

beforeEach(() => {
  useAgentsStore.getState().reset()
  vi.mocked(getAgentApi).mockReset()
})

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

  test('createAgent keeps the modal open when the container start fails', async () => {
    useAgentsStore.setState({ createModalOpen: true })
    vi.mocked(getAgentApi).mockReturnValue({
      createAgent: vi.fn().mockResolvedValue({
        ok: true,
        agent: { id: 'a9', name: 'CLI Worker', cliTool: 'claude', status: 'idle' },
      }),
      startAgent: vi.fn().mockResolvedValue({ ok: false, error: 'no runtime' }),
    } as never)

    const created = await useAgentsStore
      .getState()
      .createAgent({ name: 'CLI Worker', kind: 'cli', cliTool: 'claude' })

    expect(created).toBe(true)
    const state = useAgentsStore.getState()
    // The modal is the only surface rendering this error — closing it would
    // discard the message and leave an unexplained offline agent.
    expect(state.createModalOpen).toBe(true)
    expect(state.error).toContain('container start failed')
    expect(state.agents).toHaveLength(1)
  })

  test('createAgent closes the modal when the container starts', async () => {
    useAgentsStore.setState({ createModalOpen: true })
    vi.mocked(getAgentApi).mockReturnValue({
      createAgent: vi.fn().mockResolvedValue({
        ok: true,
        agent: { id: 'a9', name: 'CLI Worker', cliTool: 'claude', status: 'idle' },
      }),
      startAgent: vi.fn().mockResolvedValue({ ok: true, containerId: 'c1' }),
    } as never)

    const created = await useAgentsStore
      .getState()
      .createAgent({ name: 'CLI Worker', kind: 'cli', cliTool: 'claude' })

    expect(created).toBe(true)
    expect(useAgentsStore.getState().createModalOpen).toBe(false)
    expect(useAgentsStore.getState().error).toBeNull()
  })
})
