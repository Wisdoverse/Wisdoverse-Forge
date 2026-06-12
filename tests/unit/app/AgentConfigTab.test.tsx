import { render, screen, fireEvent, waitFor, cleanup, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AgentConfigTab } from '@app/features/agents/AgentConfigTab'
import { useAgentsStore } from '@app/entities/agent'

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
          runtimeId: 'af-claude-container-123',
        },
        {
          id: 'host1',
          name: 'Local Agent',
          provider: 'Codex',
          model: 'codex',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          cliTool: 'codex' as const,
          runtimeId: 'host-local-123',
          runtimeKind: 'cli' as const,
          projectName: 'Platform',
        },
        {
          id: 'future-tool',
          name: 'Future Tool Agent',
          provider: 'Work tool needs review',
          model: 'Work tool needs review',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          cliTool: 'future_tool' as never,
          runtimeId: 'af-future-tool-container-123',
          runtimeKind: 'container' as const,
        },
        {
          id: 'future-provider',
          name: 'Future Provider Agent',
          provider: 'future_provider',
          model: 'future-model-v1',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          systemPrompt: 'plain instructions',
        },
      ],
      updateAgentSystemPrompt,
    } as never)
  })

  it('shows existing system_prompt value in the textarea', () => {
    render(<AgentConfigTab agentId="a1" />)
    expect(screen.getByLabelText(/instructions for this agent/i)).toHaveValue('old prompt')
  })

  it('summarizes agent instruction readiness', () => {
    render(<AgentConfigTab agentId="a1" />)
    const summary = screen.getByTestId('agent-config-summary')
    expect(within(summary).getByText('Words')).toBeDefined()
    expect(within(summary).getByText('2')).toBeDefined()
    expect(within(summary).getByText('Lines')).toBeDefined()
    expect(within(summary).getByText('1')).toBeDefined()
    expect(within(summary).getByText('Characters')).toBeDefined()
    expect(screen.getByText('Has instructions')).toBeDefined()
    expect(screen.getByText('Agent instructions')).toBeDefined()
    expect(screen.queryByText('Prompt profile')).toBeNull()
  })

  it('Save button disabled until user edits the value', () => {
    render(<AgentConfigTab agentId="a1" />)
    const save = screen.getByRole('button', { name: /save/i })
    expect(save).toBeDisabled()
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: 'new prompt' },
    })
    expect(save).not.toBeDisabled()
  })

  it('explains instruction editing in beginner language', () => {
    render(<AgentConfigTab agentId="a1" />)
    const instructions = screen.getByLabelText(/instructions for this agent/i)

    expect(
      screen.getByText(/start from a template or write everyday instructions/i)
    ).toBeInTheDocument()
    expect(instructions).toHaveAccessibleDescription(/tell this agent the outcome/i)
    expect(screen.getByRole('status')).toHaveTextContent(
      /this agent already has saved instructions/i
    )
    expect(screen.queryByText(/system prompt/i)).toBeNull()
  })

  it('does not expose raw AI service slugs in instruction setup', () => {
    render(<AgentConfigTab agentId="future-provider" />)

    expect(screen.getByText(/AI service needs review/i)).toBeInTheDocument()
    expect(screen.getByText(/AI model selected/i)).toBeInTheDocument()
    expect(screen.queryByText(/future_provider/i)).toBeNull()
    expect(screen.queryByText(/future provider/i)).toBeNull()
    expect(screen.queryByText(/future-model-v1/i)).toBeNull()
  })

  it('applies a prompt template and can reset the edit', () => {
    render(<AgentConfigTab agentId="a1" />)
    const reviewTemplate = screen.getByRole('button', { name: /review/i })
    expect(reviewTemplate).toHaveAttribute('aria-pressed', 'false')
    fireEvent.click(reviewTemplate)

    expect(
      (screen.getByLabelText(/instructions for this agent/i) as HTMLTextAreaElement).value
    ).toContain('code review agent')
    expect(reviewTemplate).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByText('Unsaved')).toBeDefined()
    expect(screen.getByRole('status')).toHaveTextContent(/unsaved changes/i)

    fireEvent.click(screen.getByRole('button', { name: /reset/i }))
    expect(screen.getByLabelText(/instructions for this agent/i)).toHaveValue('old prompt')
  })

  it('calls updateAgentSystemPrompt with trimmed value on Save', async () => {
    updateAgentSystemPrompt.mockResolvedValue(true)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: '  new prompt  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))
    await waitFor(() => expect(updateAgentSystemPrompt).toHaveBeenCalledWith('a1', 'new prompt'))
  })

  it('shows a plain-language save failure message', async () => {
    updateAgentSystemPrompt.mockResolvedValue(false)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: 'new prompt' },
    })

    fireEvent.click(screen.getByRole('button', { name: /save/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/agent instructions were not saved/i)
    )
    expect(screen.getByRole('alert')).toHaveTextContent(/confirm it is still a chat-only agent/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/text-only model/i)
    expect(screen.getByRole('alert')).toHaveTextContent(/ask an admin to check your agent access/i)
  })

  it('empty string clears the prompt (sent as "" to backend)', async () => {
    updateAgentSystemPrompt.mockResolvedValue(true)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: '' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))
    await waitFor(() => expect(updateAgentSystemPrompt).toHaveBeenCalledWith('a1', ''))
  })

  it('renders CLI-agent-not-supported notice instead of the form', () => {
    render(<AgentConfigTab agentId="cli1" />)
    expect(screen.queryByLabelText(/instructions for this agent/i)).toBeNull()
    expect(screen.getByTestId('agent-cli-config-summary')).toBeInTheDocument()
    expect(screen.getByText('Where this agent works')).toBeInTheDocument()
    expect(screen.getByText('Claude')).toBeInTheDocument()
    expect(screen.getAllByText('Managed workspace').length).toBeGreaterThan(0)
    expect(screen.getByText('Connection')).toBeInTheDocument()
    expect(screen.getByText('Ready in managed workspace')).toBeInTheDocument()
    expect(screen.getByText('Starting project')).toBeInTheDocument()
    expect(screen.getByText('Choose a project from the sidebar first.')).toBeInTheDocument()
    expect(screen.getByText('Starting folder')).toBeInTheDocument()
    expect(screen.getByText('Workspace project folder')).toBeInTheDocument()
    expect(screen.queryByText('Connection ID')).toBeNull()
    expect(screen.queryByText('No starting project selected')).toBeNull()
    expect(screen.queryByText('claude')).toBeNull()
    expect(screen.queryByText('af-claude-container-123')).toBeNull()
    expect(screen.queryByText('/workspace')).toBeNull()
    expect(
      screen.getByText(/confirm where it can open files before assigning work/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
    expect(screen.queryByText(/work profile/i)).toBeNull()
  })

  it('explains agents connected from this computer without exposing runtime ids', () => {
    render(<AgentConfigTab agentId="host1" />)

    expect(screen.getByTestId('agent-cli-config-summary')).toBeInTheDocument()
    expect(screen.getByText('This computer')).toBeInTheDocument()
    expect(screen.getByText('Connected from this computer')).toBeInTheDocument()
    expect(screen.getByText('Starting project')).toBeInTheDocument()
    expect(screen.getByText('Platform')).toBeInTheDocument()
    expect(screen.getByText('Starting folder')).toBeInTheDocument()
    expect(screen.getByText('Folder where you ran the command')).toBeInTheDocument()
    expect(screen.queryByText('host-local-123')).toBeNull()
    expect(screen.queryByText(/runtime/i)).toBeNull()
  })

  it('labels unknown work tools without exposing raw tool values', () => {
    render(<AgentConfigTab agentId="future-tool" />)

    expect(screen.getByTestId('agent-cli-config-summary')).toBeInTheDocument()
    expect(screen.getByText('Work tool needs review')).toBeInTheDocument()
    expect(screen.queryByText('future_tool')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  it('renders "Agent not found" for unknown id', () => {
    render(<AgentConfigTab agentId="missing" />)
    expect(screen.getByText(/agent not found/i)).toBeInTheDocument()
  })
})
