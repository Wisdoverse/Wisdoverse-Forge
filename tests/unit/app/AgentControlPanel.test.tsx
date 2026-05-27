import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { AgentControlPanel } from '@app/features/agents/AgentControlPanel'
import { useAgentsStore, type AgentInfo } from '@app/shared/model/agents.store'

const agent: AgentInfo = {
  id: 'agent-1',
  name: 'Build Agent',
  provider: 'OpenAI',
  model: 'codex',
  status: 'idle',
  tasksCompleted: 0,
  tasksInProgress: 0,
  successRate: 100,
}

const sendPromptMock = vi.fn().mockResolvedValue(true)
const startAgentMock = vi.fn().mockResolvedValue(true)
const restartAgentMock = vi.fn().mockResolvedValue(true)
const deleteAgentMock = vi.fn().mockResolvedValue(true)
const originalSendPrompt = useAgentsStore.getState().sendPrompt
const originalStartAgent = useAgentsStore.getState().startAgent
const originalRestartAgent = useAgentsStore.getState().restartAgent
const originalDeleteAgent = useAgentsStore.getState().deleteAgent

beforeEach(() => {
  sendPromptMock.mockClear()
  startAgentMock.mockClear()
  restartAgentMock.mockClear()
  deleteAgentMock.mockClear()
  sendPromptMock.mockResolvedValue(true)
  useAgentsStore.setState({
    error: null,
    sendPrompt: sendPromptMock,
    startAgent: startAgentMock,
    restartAgent: restartAgentMock,
    deleteAgent: deleteAgentMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useAgentsStore.setState({
    error: null,
    sendPrompt: originalSendPrompt,
    startAgent: originalStartAgent,
    restartAgent: originalRestartAgent,
    deleteAgent: originalDeleteAgent,
  })
})

describe('AgentControlPanel', () => {
  test('keeps prompt sending actionable and focuses the missing prompt', async () => {
    render(<AgentControlPanel agent={agent} onDeleted={vi.fn()} />)

    const sendButton = screen.getByRole('button', { name: /^send$/i })
    expect(sendButton).not.toBeDisabled()

    fireEvent.click(sendButton)

    expect(sendPromptMock).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Write an instruction before sending it to this agent.'
    )
    const promptInput = screen.getByLabelText(/send prompt/i)
    expect(promptInput).toHaveFocus()

    fireEvent.change(promptInput, { target: { value: ' Check repository status ' } })

    expect(screen.queryByRole('alert')).toBeNull()
    fireEvent.click(sendButton)

    await waitFor(() =>
      expect(sendPromptMock).toHaveBeenCalledWith('agent-1', 'Check repository status')
    )
    await waitFor(() => expect(promptInput).toHaveValue(''))
  })
})
