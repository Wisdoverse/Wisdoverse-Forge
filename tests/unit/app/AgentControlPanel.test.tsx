import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { AgentControlPanel } from '@app/features/agents/AgentControlPanel'
import { useAgentsStore, type AgentInfo } from '@app/entities/agent'

const sendPromptMock = vi.fn()
const startAgentMock = vi.fn()
const restartAgentMock = vi.fn()
const deleteAgentMock = vi.fn()

const containerAgent: AgentInfo = {
  id: 'agent-1',
  name: 'Starter Agent',
  provider: 'Anthropic',
  model: 'claude',
  status: 'idle',
  tasksCompleted: 4,
  tasksInProgress: 0,
  successRate: 0.95,
  cliTool: 'claude',
  containerId: 'container-123',
  workspaceId: 'workspace-1',
  workspaceName: 'Default workspace',
  projectId: 'project-1',
  projectName: 'Default project',
}

const textOnlyAgent: AgentInfo = {
  ...containerAgent,
  id: 'text-agent',
  name: 'Text Agent',
  cliTool: undefined,
  containerId: undefined,
}

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

beforeEach(() => {
  useAgentsStore.getState().reset()
  sendPromptMock.mockResolvedValue(true)
  startAgentMock.mockResolvedValue(true)
  restartAgentMock.mockResolvedValue(true)
  deleteAgentMock.mockResolvedValue(true)
  useAgentsStore.setState({
    error: null,
    sendPrompt: sendPromptMock,
    startAgent: startAgentMock,
    restartAgent: restartAgentMock,
    deleteAgent: deleteAgentMock,
  } as never)
})

describe('AgentControlPanel', () => {
  test('turns action failures into a clear recovery path', () => {
    useAgentsStore.setState({ error: 'Start request failed' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(/refresh this agent/i)
    expect(screen.getByRole('alert')).toHaveTextContent(/wait for idle or working/i)
    expect(screen.getByRole('alert')).toHaveTextContent(/ask an admin to check your access/i)
  })

  test('explains quick messages and sends trimmed text', async () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByLabelText(/send a message/i)).toBeDefined()
    expect(screen.getByText(/for tracked work, create a task/i)).toBeDefined()

    fireEvent.change(screen.getByLabelText(/send a message/i), {
      target: { value: '  Check the latest run  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /send message/i }))

    await waitFor(() => {
      expect(sendPromptMock).toHaveBeenCalledWith('agent-1', 'Check the latest run')
    })
    expect(screen.getByLabelText(/send a message/i)).toHaveValue('')
  })

  test('uses model service language for text-only agents', () => {
    render(<AgentControlPanel agent={textOnlyAgent} onDeleted={() => {}} />)

    expect(screen.getByText('Text-only model controls')).toBeDefined()
    expect(screen.getByText(/saved model service/i)).toBeDefined()
    expect(screen.queryByText(/provider setup/i)).toBeNull()
  })

  test('shows start guidance for pending agent workspaces', async () => {
    render(
      <AgentControlPanel
        agent={{ ...containerAgent, id: 'pending-agent', containerId: undefined }}
        onDeleted={() => {}}
      />
    )

    expect(screen.getByText('Agent workspace needs to start')).toBeDefined()
    expect(screen.getByText(/has no running workspace yet/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /start agent/i }))

    await waitFor(() => {
      expect(startAgentMock).toHaveBeenCalledWith('pending-agent')
    })
  })

  test('warns before restarting a running agent workspace', async () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    expect(screen.getByText('Restart this agent?')).toBeDefined()
    expect(screen.getByText(/active work may stop/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /keep running/i }))
    expect(restartAgentMock).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /restart now/i }))

    await waitFor(() => {
      expect(restartAgentMock).toHaveBeenCalledWith('agent-1')
    })
  })

  test('warns before deleting and calls the close handler only after success', async () => {
    const onDeleted = vi.fn()
    render(<AgentControlPanel agent={containerAgent} onDeleted={onDeleted} />)

    fireEvent.click(screen.getByRole('button', { name: /delete agent/i }))
    expect(screen.getByText('Delete this agent?')).toBeDefined()
    expect(screen.getByText(/removes the agent from future work/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /keep agent/i }))
    expect(deleteAgentMock).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: /delete agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /delete permanently/i }))

    await waitFor(() => {
      expect(deleteAgentMock).toHaveBeenCalledWith('agent-1')
      expect(onDeleted).toHaveBeenCalledTimes(1)
    })
  })
})
