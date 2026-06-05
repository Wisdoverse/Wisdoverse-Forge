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
    useAgentsStore.setState({ error: 'HTTP 500: Start request failed' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(/refresh this agent/i)
    expect(screen.getByRole('alert')).toHaveTextContent(/forge could not update this agent/i)
    expect(screen.getByRole('alert')).toHaveTextContent(
      /ask an owner or admin to check Agent Work Setup/i
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 500/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Start request failed/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/temporarily unavailable/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/agent service/i)
  })

  test('turns busy action failures into a wait and refresh step', () => {
    useAgentsStore.setState({ error: 'HTTP 429: rate limit exceeded' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'The agent controls are busy. Wait a moment, refresh this agent, then try again.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 429/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/agent service/i)
  })

  test('keeps generic recovery steps aligned with Ready status wording', () => {
    useAgentsStore.setState({ error: 'Unexpected control result' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(/wait for Ready or Working/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/wait for Idle/i)
  })

  test('explains quick messages and sends trimmed text', async () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    const instructionInput = screen.getByLabelText(/send one instruction/i)
    expect(instructionInput).toBeDefined()
    expect(screen.getByText(/for work that needs a clear result, create a task/i)).toBeDefined()
    expect(instructionInput).toHaveAccessibleDescription(
      /send one concrete instruction, then watch this agent's history for progress/i
    )

    fireEvent.change(instructionInput, {
      target: { value: '  Check the latest run  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /send instruction/i }))

    await waitFor(() => {
      expect(sendPromptMock).toHaveBeenCalledWith('agent-1', 'Check the latest run')
    })
    expect(screen.getByLabelText(/send one instruction/i)).toHaveValue('')
  })

  test('focuses the instruction box when users try to send blank text', () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    const instructionInput = screen.getByLabelText(/send one instruction/i)
    fireEvent.click(screen.getByRole('button', { name: /send instruction/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Write an instruction before sending it to this agent.'
    )
    expect(instructionInput).toHaveFocus()
    expect(instructionInput).toHaveAttribute(
      'aria-describedby',
      expect.stringContaining('agent-control-prompt-error')
    )
    expect(sendPromptMock).not.toHaveBeenCalled()
  })

  test('uses chat-only language for agents that answer through an AI service', () => {
    render(<AgentControlPanel agent={textOnlyAgent} onDeleted={() => {}} />)

    expect(screen.getByText('Chat-only agent controls')).toBeDefined()
    expect(screen.getByText(/connected AI service/i)).toBeDefined()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
    expect(screen.queryByText(/model service/i)).toBeNull()
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
    expect(screen.getByText(/opening its live work window/i)).toBeDefined()
    expect(screen.queryByText(/opening a terminal/i)).toBeNull()
    expect(screen.queryByText(/opening the command window/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /start agent/i }))

    await waitFor(() => {
      expect(startAgentMock).toHaveBeenCalledWith('pending-agent')
    })
  })

  test('warns before restarting a running agent workspace', async () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    expect(screen.getByText('Restart this agent?')).toBeDefined()
    expect(screen.getByText(/live work window or task updates are stuck/i)).toBeDefined()
    expect(screen.getByText(/active work may stop/i)).toBeDefined()
    expect(screen.queryByText(/terminal or task updates/i)).toBeNull()
    expect(screen.queryByText(/command window or task updates/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /keep running/i }))
    expect(restartAgentMock).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /restart now/i }))

    await waitFor(() => {
      expect(restartAgentMock).toHaveBeenCalledWith('agent-1')
    })
  })

  test('warns before removing and calls the close handler only after success', async () => {
    const onDeleted = vi.fn()
    render(<AgentControlPanel agent={containerAgent} onDeleted={onDeleted} />)

    fireEvent.click(screen.getByRole('button', { name: /remove agent/i }))
    expect(screen.getByText('Remove this agent?')).toBeDefined()
    expect(screen.getByText(/removes the agent from future work/i)).toBeDefined()
    expect(screen.getByText(/existing task history stays available/i)).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /keep agent/i }))
    expect(deleteAgentMock).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: /remove agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /^remove agent$/i }))

    await waitFor(() => {
      expect(deleteAgentMock).toHaveBeenCalledWith('agent-1')
      expect(onDeleted).toHaveBeenCalledTimes(1)
    })
  })
})
