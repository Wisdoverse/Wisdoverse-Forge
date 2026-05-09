import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AgentConfigTab } from '@app/features/agents/AgentConfigTab'
import { useAgentsStore } from '@app/shared/model/agents.store'

afterEach(cleanup)

describe('AgentConfigTab', () => {
  const updateAgentSystemPrompt = vi.fn()

  beforeEach(() => {
    vi.clearAllMocks()
    useAgentsStore.getState().reset()
    useAgentsStore.setState({
      agents: [
        {
          id: 'a1',
          name: 'Provider Agent',
          provider: 'anthropic',
          model: 'claude-sonnet-4-6',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          systemPrompt: 'old prompt',
        },
        {
          id: 'cli1',
          name: 'CLI Agent',
          provider: 'Anthropic',
          model: 'claude',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          cliTool: 'claude' as const,
        },
      ],
      updateAgentSystemPrompt,
    } as never)
  })

  it('shows existing system_prompt value in the textarea', () => {
    render(<AgentConfigTab agentId="a1" />)
    expect(screen.getByLabelText(/system prompt/i)).toHaveValue('old prompt')
  })

  it('Save button disabled until user edits the value', () => {
    render(<AgentConfigTab agentId="a1" />)
    const save = screen.getByRole('button', { name: /save/i })
    expect(save).toBeDisabled()
    fireEvent.change(screen.getByLabelText(/system prompt/i), {
      target: { value: 'new prompt' },
    })
    expect(save).not.toBeDisabled()
  })

  it('calls updateAgentSystemPrompt with trimmed value on Save', async () => {
    updateAgentSystemPrompt.mockResolvedValue(true)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/system prompt/i), {
      target: { value: '  new prompt  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))
    await waitFor(() => expect(updateAgentSystemPrompt).toHaveBeenCalledWith('a1', 'new prompt'))
  })

  it('empty string clears the prompt (sent as "" to backend)', async () => {
    updateAgentSystemPrompt.mockResolvedValue(true)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/system prompt/i), {
      target: { value: '' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))
    await waitFor(() => expect(updateAgentSystemPrompt).toHaveBeenCalledWith('a1', ''))
  })

  it('renders CLI-agent-not-supported notice instead of the form', () => {
    render(<AgentConfigTab agentId="cli1" />)
    expect(screen.queryByLabelText(/system prompt/i)).toBeNull()
    expect(screen.getByText(/only available for provider\+prompt agents/i)).toBeInTheDocument()
  })

  it('renders "Agent not found" for unknown id', () => {
    render(<AgentConfigTab agentId="missing" />)
    expect(screen.getByText(/agent not found/i)).toBeInTheDocument()
  })
})
