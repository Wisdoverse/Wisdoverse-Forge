import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  runtimeKind: 'api',
}

const offlineTextOnlyAgent: AgentInfo = {
  ...textOnlyAgent,
  id: 'offline-text-agent',
  status: 'offline',
}

const hostCliAgent: AgentInfo = {
  ...containerAgent,
  id: 'host-agent',
  name: 'Laptop Agent',
  containerId: undefined,
  runtimeId: 'host-aabbccdd',
  runtimeKind: 'cli',
}

const offlineHostCliAgent: AgentInfo = {
  ...hostCliAgent,
  id: 'offline-host-agent',
  status: 'offline',
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

    expect(screen.getByRole('alert')).toHaveTextContent(/Open Agents, choose this agent again/i)
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Read the next line, then run the agent action again.'
    )
    expect(screen.getByRole('alert')).toHaveTextContent(/Forge could not finish the change/i)
    expect(screen.getByRole('alert')).toHaveTextContent(
      /ask an owner or admin to check this agent's connection and access in Agents/i
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Follow the step below/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/check this agent setup/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 500/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Start request failed/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/temporarily unavailable/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/agent service/i)
  })

  test('turns busy action failures into a wait and agent-selection step', () => {
    useAgentsStore.setState({ error: 'HTTP 429: rate limit exceeded' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Wait a moment, then open Agents and choose this agent again. The agent controls are busy.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 429/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/agent service/i)
  })

  test('turns permission failures into agent management guidance', () => {
    useAgentsStore.setState({ error: 'HTTP 403: Forbidden' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Ask an owner or admin to let you manage this agent, then return to Agents and run the agent action again. You do not have permission to change this agent.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/then try again/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/update what you can do/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 403/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Forbidden/i)
  })

  test('turns changed-agent failures into a concrete status check', () => {
    useAgentsStore.setState({ error: 'HTTP 409: Conflict' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Open Agents, choose this agent again, check whether it is Ready, Working, or Offline, then run the agent action again. This agent changed while you were working.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/then try again/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 409/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Conflict/i)
  })

  test('turns connection failures into a clear agent-selection step', () => {
    useAgentsStore.setState({ error: 'Failed to fetch' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Check your connection, then open Agents and choose this agent again. Forge could not connect while changing this agent.'
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/Failed to fetch/i)
  })

  test('keeps generic recovery steps aligned with Ready status wording', () => {
    useAgentsStore.setState({ error: 'Unexpected control result' } as never)

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    expect(screen.getByRole('alert')).toHaveTextContent(/wait for Ready or Working/i)
    expect(screen.getByRole('alert')).toHaveTextContent(/check your agent access/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/wait for Idle/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/check what you can do/i)
  })

  test('explains quick messages and sends trimmed text', async () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    const messageInput = screen.getByLabelText(/send a quick message/i)
    expect(messageInput).toBeDefined()
    expect(screen.getByText(/for work that needs a clear result, create a task/i)).toBeDefined()
    expect(messageInput).toHaveAccessibleDescription(
      /send one concrete message, then watch this agent's history for progress/i
    )
    expect(screen.queryByLabelText(/send one instruction/i)).toBeNull()

    fireEvent.change(messageInput, {
      target: { value: '  Check the latest run  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^send message$/i }))

    await waitFor(() => {
      expect(sendPromptMock).toHaveBeenCalledWith('agent-1', 'Check the latest run')
    })
    expect(screen.getByLabelText(/send a quick message/i)).toHaveValue('')
    expect(await screen.findByRole('status')).toHaveTextContent(
      "Message sent. Watch this agent's history for progress"
    )
    expect(screen.getByRole('status')).toHaveTextContent(
      'create a task next time when you need a tracked result others can find later'
    )
  })

  test('names message send progress while the request is running', async () => {
    let finishSend: (sent: boolean) => void = () => undefined
    sendPromptMock.mockReturnValueOnce(
      new Promise<boolean>((resolve) => {
        finishSend = resolve
      })
    )

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    fireEvent.change(screen.getByLabelText(/send a quick message/i), {
      target: { value: 'Check recent work' },
    })
    fireEvent.click(screen.getByRole('button', { name: /^send message$/i }))

    expect(screen.getByRole('button', { name: /sending message/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Sending\.\.\.$/i })).toBeNull()

    await act(async () => {
      finishSend(true)
    })
  })

  test('keeps the message box usable when sending fails unexpectedly', async () => {
    sendPromptMock.mockRejectedValueOnce(new Error('socket hang up'))

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    const messageInput = screen.getByLabelText(/send a quick message/i)
    const sendButton = screen.getByRole('button', { name: /^send message$/i })
    fireEvent.change(messageInput, { target: { value: 'Check recent work' } })
    fireEvent.click(sendButton)

    await waitFor(() => expect(sendButton).not.toBeDisabled())

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Action did not finish')
    expect(alert).toHaveTextContent('Read the next line, then run the agent action again.')
    expect(alert).not.toHaveTextContent(/Follow the step below/i)
    expect(alert).toHaveTextContent(
      /Open Agents, choose this agent again, confirm it still shows Ready/i
    )
    expect(alert).toHaveTextContent(/create a task instead/i)
    expect(alert).toHaveTextContent(/ask an owner or admin to check agent messaging/i)
    expect(alert).not.toHaveTextContent(/socket hang up/i)
    expect(messageInput).toHaveValue('Check recent work')
  })

  test('focuses the message box when users try to send blank text', () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    const messageInput = screen.getByLabelText(/send a quick message/i)
    fireEvent.click(screen.getByRole('button', { name: /^send message$/i }))

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Write a message before sending it to this agent.')
    expect(alert).not.toHaveTextContent('Write an instruction')
    expect(messageInput).toHaveFocus()
    expect(messageInput).toHaveAttribute(
      'aria-describedby',
      expect.stringContaining('agent-control-prompt-error')
    )
    expect(sendPromptMock).not.toHaveBeenCalled()
  })

  test('uses chat-only language for agents that answer through an AI service', () => {
    render(<AgentControlPanel agent={textOnlyAgent} onDeleted={() => {}} />)

    expect(screen.getByText('Chat-only AI service controls')).toBeDefined()
    expect(screen.getByText(/replies through its AI service/i)).toBeDefined()
    expect(screen.queryByText('Chat-only agent controls')).toBeNull()
    expect(screen.getByText('Ready for chat and tracked tasks')).toBeDefined()
    expect(screen.getByText(/question answered, writing help, or a result check/i)).toBeDefined()
    expect(screen.queryByText(/planning or review with a clear result/i)).toBeNull()
    expect(screen.queryByText(/No recovery action needed/i)).toBeNull()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
    expect(screen.queryByText(/model service/i)).toBeNull()
    expect(screen.queryByText(/provider setup/i)).toBeNull()
  })

  test('disables quick messages when a chat-only agent is offline', () => {
    render(<AgentControlPanel agent={offlineTextOnlyAgent} onDeleted={() => {}} />)

    expect(screen.getByText('AI service needs a check')).toBeDefined()
    expect(screen.getByText('Check AI service before sending')).toBeDefined()
    expect(screen.getAllByText(/choose Check connection for this service/i).length).toBe(4)
    expect(screen.getAllByText(/return to Agents and choose this agent again/i).length).toBe(4)
    expect(screen.queryByText(/Settings > AI services/i)).toBeNull()
    expect(screen.queryByText('Ready for chat and tracked tasks')).toBeNull()
    expect(screen.queryByText('Chat-only AI service is offline')).toBeNull()
    expect(screen.queryByText(/This chat-only agent is not connected/i)).toBeNull()
    expect(screen.queryByText(/click Check/i)).toBeNull()
    expect(screen.queryByText(/refresh Agents/i)).toBeNull()

    const messageInput = screen.getByLabelText(/send a quick message/i)
    expect(messageInput).toBeDisabled()
    expect(messageInput).toHaveAttribute(
      'placeholder',
      'Check this AI service before sending a message.'
    )
    expect(messageInput).toHaveAccessibleDescription(
      /choose Check connection for this service/i
    )
    expect(screen.getByRole('button', { name: /send message/i })).toBeDisabled()
    expect(sendPromptMock).not.toHaveBeenCalled()
  })

  test('guides joined-computer agents without start or restart controls', () => {
    render(<AgentControlPanel agent={hostCliAgent} onDeleted={() => {}} />)

    expect(screen.getByText('This computer is connected')).toBeDefined()
    expect(screen.getByText(/this computer is already connected/i)).toBeDefined()
    expect(screen.getByText(/close that window only when you want it offline/i)).toBeDefined()
    expect(screen.queryByText(/setup command/i)).toBeNull()
    expect(screen.queryByText(/connection command/i)).toBeNull()
    expect(screen.queryByText(/bring it online/i)).toBeNull()
    expect(screen.getByText('Keep this computer online')).toBeDefined()
    expect(
      screen.getByText(
        'Keep Terminal or PowerShell open while it works. Use this page for quick messages, tracked tasks, or cleanup.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/command app/i)).toBeNull()
    expect(screen.queryByRole('button', { name: /start agent/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /restart agent/i })).toBeNull()
    expect(screen.queryByText(/No recovery action needed/i)).toBeNull()
  })

  test('explains how to reconnect a joined-computer agent when it is offline', () => {
    render(<AgentControlPanel agent={offlineHostCliAgent} onDeleted={() => {}} />)

    expect(screen.getByText('Reconnect this computer from Agents')).toBeDefined()
    expect(screen.getByText(/use Back to return to Agents/i)).toBeDefined()
    expect(screen.getAllByText(/choose Connect this computer/i).length).toBeGreaterThan(0)
    expect(screen.getByText('Use Connect this computer')).toBeDefined()
    expect(screen.getByText(/copy the new setup text from Agents/i)).toBeDefined()
    expect(screen.getByText(/paste it in Terminal or PowerShell on that computer/i)).toBeDefined()
    expect(screen.getByText(/come back here to send messages or tasks/i)).toBeDefined()
    expect(screen.queryByText('This computer is offline')).toBeNull()
    expect(
      screen.queryByText(/paste the setup text in that computer's command app again/i)
    ).toBeNull()
    expect(screen.queryByText('Paste setup text to reconnect')).toBeNull()
    expect(screen.queryByText(/paste the setup text again, then come back here/i)).toBeNull()
    expect(screen.queryByText(/setup command/i)).toBeNull()
    expect(screen.queryByText(/already connected/i)).toBeNull()
    expect(screen.queryByRole('button', { name: /start agent/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /restart agent/i })).toBeNull()
    expect(screen.getByLabelText(/send a quick message/i)).toBeDisabled()
    expect(screen.getByRole('button', { name: /send message/i })).toBeDisabled()
  })

  test('shows start guidance for pending agent workspaces', async () => {
    let finishStart: (started: boolean) => void = () => undefined
    startAgentMock.mockReturnValueOnce(
      new Promise<boolean>((resolve) => {
        finishStart = resolve
      })
    )

    render(
      <AgentControlPanel
        agent={{ ...containerAgent, id: 'pending-agent', containerId: undefined }}
        onDeleted={() => {}}
      />
    )

    expect(screen.getByText('File work needs to start')).toBeDefined()
    expect(screen.getByText(/file work has not started yet/i)).toBeDefined()
    expect(screen.getByText(/Wait for Ready before sending file work/i)).toBeDefined()
    expect(
      screen.getByText(/Start file work before sending file tasks or opening Live work/i)
    ).toBeDefined()
    expect(screen.queryByText(/opening a terminal/i)).toBeNull()
    expect(screen.queryByText(/opening the command window/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /start file work/i }))

    expect(screen.getByRole('button', { name: /starting file work/i })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /^Starting\.\.\.$/i })).toBeNull()

    await act(async () => {
      finishStart(true)
    })

    await waitFor(() => {
      expect(startAgentMock).toHaveBeenCalledWith('pending-agent')
    })
    expect(await screen.findByRole('status')).toHaveTextContent('File work start requested')
    expect(screen.getByRole('status')).toHaveTextContent(
      'Go back to Agents, choose this agent again when it shows Ready'
    )
    expect(screen.getByRole('status')).not.toHaveTextContent(/Refresh Agents/i)
  })

  test('recovers the start control when a pending workspace does not start', async () => {
    startAgentMock.mockResolvedValueOnce(false)

    render(
      <AgentControlPanel
        agent={{ ...containerAgent, id: 'pending-start-failed', containerId: undefined }}
        onDeleted={() => {}}
      />
    )

    const startButton = screen.getByRole('button', { name: /start file work/i })
    fireEvent.click(startButton)

    await waitFor(() => expect(startButton).not.toBeDisabled())

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Action did not finish')
    expect(alert).toHaveTextContent(
      /Go back to Agents, choose this agent again, then choose Start file work again/i
    )
    expect(alert).toHaveTextContent(/ask an owner or admin to check Where agents work/i)
    expect(alert).not.toHaveTextContent(/Refresh Agents/i)
    expect(alert).not.toHaveTextContent(/agent control action failed/i)
    expect(screen.queryByRole('status')).toBeNull()
    expect(screen.getByRole('button', { name: /start file work/i })).toBeEnabled()
  })

  test('warns before restarting a running agent workspace', async () => {
    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    expect(screen.getByText('Restart this agent?')).toBeDefined()
    expect(screen.getByText(/Tasks or Live work stops showing progress/i)).toBeDefined()
    expect(screen.getByText(/active work may stop/i)).toBeDefined()
    expect(screen.queryByText(/terminal or task updates/i)).toBeNull()
    expect(screen.queryByText(/command window or task updates/i)).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /keep running/i }))
    expect(restartAgentMock).not.toHaveBeenCalled()

    expect(screen.getByText('Fix stuck file work')).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /restart now/i }))

    await waitFor(() => {
      expect(restartAgentMock).toHaveBeenCalledWith('agent-1')
    })
    expect(await screen.findByRole('status')).toHaveTextContent('Restart requested')
    expect(screen.getByRole('status')).toHaveTextContent(
      'Wait until this agent shows Ready before sending new work'
    )
  })

  test('returns to the restart card when restart fails unexpectedly', async () => {
    restartAgentMock.mockRejectedValueOnce(new Error('restart socket failed'))

    render(<AgentControlPanel agent={containerAgent} onDeleted={() => {}} />)

    fireEvent.click(screen.getByRole('button', { name: /restart agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /restart now/i }))

    await waitFor(() => {
      expect(restartAgentMock).toHaveBeenCalledWith('agent-1')
      expect(screen.getByText('Fix stuck file work')).toBeDefined()
    })

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Action did not finish')
    expect(alert).toHaveTextContent(/choose Restart agent again only if Tasks or Live work/i)
    expect(alert).toHaveTextContent(
      /ask an owner or admin to check this agent's connection and access in Agents/i
    )
    expect(alert).not.toHaveTextContent(/check this agent setup/i)
    expect(alert).not.toHaveTextContent(/restart socket failed/i)
    expect(screen.queryByText('Restart this agent?')).toBeNull()
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

  test('keeps the agent visible when removal fails', async () => {
    const onDeleted = vi.fn()
    deleteAgentMock.mockResolvedValueOnce(false)

    render(<AgentControlPanel agent={containerAgent} onDeleted={onDeleted} />)

    fireEvent.click(screen.getByRole('button', { name: /remove agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /^remove agent$/i }))

    await waitFor(() => expect(deleteAgentMock).toHaveBeenCalledWith('agent-1'))

    expect(onDeleted).not.toHaveBeenCalled()
    expect(screen.getByText('Remove this agent')).toBeDefined()
    expect(screen.queryByText('Remove this agent?')).toBeNull()
    expect(screen.getByRole('alert')).toHaveTextContent(/Action did not finish/i)
    expect(screen.getByRole('alert')).toHaveTextContent(
      /Open Agents, choose this agent again, then choose Remove agent again/i
    )
    expect(screen.getByRole('alert')).toHaveTextContent(
      /ask an owner or admin to check your agent access/i
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/agent control action failed/i)
  })
})
