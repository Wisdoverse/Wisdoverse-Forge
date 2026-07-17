import { render, screen, fireEvent, waitFor, cleanup, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AgentConfigTab } from '@app/features/agents/AgentConfigTab'
import { useAgentsStore } from '@app/entities/agent'

afterEach(cleanup)

describe('AgentConfigTab', () => {
  const updateAgentSystemPrompt = vi.fn()
  const oldDeliveryTemplatePhrases = [
    new RegExp(['Clarify', 'blockers', 'early'].join('\\s+'), 'i'),
    new RegExp(['validation', 'evidence'].join('\\s+'), 'i'),
    new RegExp(['each', 'handoff'].join('\\s+'), 'i'),
  ]

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
          id: 'host-disconnected',
          name: 'Disconnected Local Agent',
          provider: 'Codex',
          model: 'codex',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          cliTool: 'codex' as const,
          runtimeId: null,
          runtimeKind: 'cli' as const,
          projectName: 'Platform',
        },
        {
          id: 'future-tool',
          name: 'Future Tool Agent',
          provider: 'Check tool selected in Settings',
          model: 'Check tool selected in Settings',
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
        {
          id: 'missing-model',
          name: 'Missing Model Agent',
          provider: 'anthropic',
          model: ' ',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          systemPrompt: 'plain instructions',
        },
        {
          id: 'no-instructions',
          name: 'No Instructions Agent',
          provider: 'anthropic',
          model: 'claude-sonnet-4-6',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          systemPrompt: '',
        },
        {
          id: 'missing-tool',
          name: 'Missing Tool Agent',
          provider: 'Check tool selected in Settings',
          model: 'Check tool selected in Settings',
          status: 'idle' as const,
          tasksCompleted: 0,
          tasksInProgress: 0,
          successRate: 0,
          cliTool: ' ' as never,
          runtimeId: 'af-missing-tool-container-123',
          runtimeKind: 'container' as const,
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
    expect(screen.getByText('How this agent answers')).toBeDefined()
    expect(screen.queryByText('Agent instructions')).toBeNull()
    expect(screen.queryByText('Prompt profile')).toBeNull()
  })

  it('Save button disabled until user edits the value', () => {
    render(<AgentConfigTab agentId="a1" />)
    const save = screen.getByRole('button', { name: /save answer guidance/i })
    expect(save).toBeDisabled()
    expect(save).toHaveAttribute('title', 'Change the answer guidance before save is available.')
    expect(screen.queryByRole('button', { name: /save instructions/i })).toBeNull()
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: 'new prompt' },
    })
    expect(save).not.toBeDisabled()
    expect(save).toHaveAttribute('title', 'Save this answer guidance for future work.')
  })

  it('explains instruction editing in beginner language', () => {
    render(<AgentConfigTab agentId="a1" />)
    const instructions = screen.getByLabelText(/instructions for this agent/i)

    expect(
      screen.getByText(/start from a template or write everyday instructions/i)
    ).toBeInTheDocument()
    expect(instructions).toHaveAccessibleDescription(/tell this agent the outcome/i)
    expect(screen.getByRole('status')).toHaveTextContent(/this agent already has guidance saved/i)
    expect(screen.queryByText(/this agent already has saved instructions/i)).toBeNull()
    expect(screen.queryByText(/system prompt/i)).toBeNull()
  })

  it('points empty instruction setup to the next action', () => {
    render(<AgentConfigTab agentId="no-instructions" />)

    expect(screen.getByText('Add instructions')).toBeInTheDocument()
    expect(screen.getByRole('status')).toHaveTextContent(
      /choose a template or write instructions before saving/i
    )
    expect(screen.queryByText('No instructions')).toBeNull()
  })

  it('does not expose raw AI service slugs in instruction setup', () => {
    render(<AgentConfigTab agentId="future-provider" />)

    expect(screen.getByText(/Check AI service/i)).toBeInTheDocument()
    expect(screen.getByText(/AI service choice selected/i)).toBeInTheDocument()
    expect(screen.queryByText(/AI model selected/i)).toBeNull()
    expect(screen.queryByText(/future_provider/i)).toBeNull()
    expect(screen.queryByText(/future provider/i)).toBeNull()
    expect(screen.queryByText(/future-model-v1/i)).toBeNull()
  })

  it('tells users to check setup when a chat-only agent has no model details', () => {
    render(<AgentConfigTab agentId="missing-model" />)

    expect(screen.getByText(/Check AI service choice/i)).toBeInTheDocument()
    expect(screen.queryByText(/Check AI model/i)).toBeNull()
    expect(screen.queryByText(/Check AI model setup/i)).toBeNull()
    expect(screen.queryByText(/AI model not reported/i)).toBeNull()
    expect(screen.queryByText(/Model not reported/i)).toBeNull()
  })

  it('applies a prompt template and can reset the edit', () => {
    render(<AgentConfigTab agentId="a1" />)
    const reviewTemplate = screen.getByRole('button', { name: /check results/i })
    expect(reviewTemplate).toHaveAttribute('aria-pressed', 'false')
    fireEvent.click(reviewTemplate)

    expect(
      (screen.getByLabelText(/instructions for this agent/i) as HTMLTextAreaElement).value
    ).toContain('check work before the team uses it')
    expect(reviewTemplate).toHaveAttribute('aria-pressed', 'true')
    expect(screen.queryByRole('button', { name: /^review$/i })).toBeNull()
    expect(screen.getByText('Unsaved')).toBeDefined()
    expect(screen.getByRole('status')).toHaveTextContent(/unsaved changes/i)
    expect(screen.getByRole('button', { name: /reset/i })).toHaveAttribute(
      'title',
      'Reset to the last saved version.'
    )
    expect(screen.getByRole('button', { name: /reset/i })).not.toHaveAttribute(
      'title',
      'Reset to the last saved instructions.'
    )

    fireEvent.click(screen.getByRole('button', { name: /reset/i }))
    expect(screen.getByLabelText(/instructions for this agent/i)).toHaveValue('old prompt')
  })

  it('uses plain-language finish-work template instructions', () => {
    render(<AgentConfigTab agentId="a1" />)
    const deliveryTemplate = screen.getByRole('button', { name: /finish work/i })
    expect(screen.queryByRole('button', { name: /^delivery$/i })).toBeNull()
    fireEvent.click(deliveryTemplate)

    const instructions = screen.getByLabelText(/instructions for this agent/i)
    expect(instructions).toHaveValue(
      'You help finish assigned work. Ask early for missing information, keep changes scoped to the task you receive, preserve existing conventions, and report what you checked before sharing results.'
    )
    expect(instructions).not.toHaveValue(expect.stringMatching(/delivery-focused/i))
    for (const phrase of oldDeliveryTemplatePhrases) {
      expect(instructions).not.toHaveValue(expect.stringMatching(phrase))
    }
  })

  it('uses beginner-facing wording in the check-results template', () => {
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.click(screen.getByRole('button', { name: /check results/i }))

    const instructions = screen.getByLabelText(
      /instructions for this agent/i
    ) as HTMLTextAreaElement
    expect(instructions.value).toContain('check work before the team uses it')
    expect(instructions.value).toContain('could break the result')
    expect(instructions.value).toContain('missing check')
    expect(instructions.value).toContain('point to the file or behavior')
    expect(instructions.value).not.toMatch(/review work carefully/i)
    expect(instructions.value).not.toMatch(/code review agent/i)
    expect(instructions.value).not.toMatch(/regressions/i)
    expect(instructions.value).not.toMatch(/missing tests/i)
    expect(instructions.value).not.toMatch(/unclear ownership/i)
    expect(instructions.value).not.toMatch(/concrete findings/i)
    expect(instructions.value).not.toMatch(/cite the exact files/i)
  })

  it('uses beginner-facing wording in the sort-work template', () => {
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.click(screen.getByRole('button', { name: /sort work/i }))

    const instructions = screen.getByLabelText(
      /instructions for this agent/i
    ) as HTMLTextAreaElement
    expect(instructions.value).toContain('sort incoming work')
    expect(instructions.value).toContain('steps the user described')
    expect(instructions.value).toContain('plain language')
    expect(instructions.value).toContain("ask for more information when it's needed")
    expect(instructions.value).not.toMatch(/triage/i)
    expect(instructions.value).not.toMatch(/reported behavior/i)
    expect(instructions.value).not.toMatch(/symptoms/i)
    expect(instructions.value).not.toMatch(/likely cause/i)
    expect(instructions.value).not.toMatch(/smallest safe fix/i)
    expect(instructions.value).not.toMatch(/root cause/i)
    expect(instructions.value).not.toMatch(/more evidence/i)
  })

  it('calls updateAgentSystemPrompt with trimmed value on Save', async () => {
    updateAgentSystemPrompt.mockResolvedValue(true)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: '  new prompt  ' },
    })
    fireEvent.click(screen.getByRole('button', { name: /save/i }))
    await waitFor(() => expect(updateAgentSystemPrompt).toHaveBeenCalledWith('a1', 'new prompt'))
    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent(/answer guidance saved/i)
    )
    expect(screen.getByRole('status')).not.toHaveTextContent(/agent instructions saved/i)
  })

  it('shows a plain-language save failure message', async () => {
    updateAgentSystemPrompt.mockResolvedValue(false)
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: 'new prompt' },
    })

    fireEvent.click(screen.getByRole('button', { name: /save/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/answer guidance was not saved/i)
    )
    expect(screen.getByRole('alert')).toHaveTextContent(/^open agents/i)
    expect(screen.getByRole('alert')).toHaveTextContent(/choose this simple chat agent again/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/text-only model/i)
    expect(screen.getByRole('alert')).toHaveTextContent(
      /ask an owner or admin to check your agent access/i
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/ask an admin/i)
  })

  it('shows the same safe save failure when saving throws', async () => {
    updateAgentSystemPrompt.mockRejectedValue(new Error('HTTP 500: database unavailable'))
    render(<AgentConfigTab agentId="a1" />)
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
      target: { value: 'new prompt' },
    })

    fireEvent.click(screen.getByRole('button', { name: /save/i }))

    await waitFor(() =>
      expect(screen.getByRole('alert')).toHaveTextContent(/answer guidance was not saved/i)
    )
    expect(screen.getByRole('alert')).not.toHaveTextContent(/HTTP 500/i)
    expect(screen.getByRole('alert')).not.toHaveTextContent(/database unavailable/i)
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
    expect(screen.getAllByText('Project files').length).toBeGreaterThan(0)
    expect(screen.getByText('Connection')).toBeInTheDocument()
    expect(screen.getByText('Ready with project files')).toBeInTheDocument()
    expect(screen.getByText('Project for new tasks')).toBeInTheDocument()
    expect(screen.getByText('Open project settings first.')).toBeInTheDocument()
    expect(screen.queryByText('Choose a project from the sidebar first.')).toBeNull()
    expect(screen.getByText('Folder agents open')).toBeInTheDocument()
    expect(screen.getByText('Default project folder')).toBeInTheDocument()
    expect(screen.queryByText('Starting project')).toBeNull()
    expect(screen.queryByText('Starting folder')).toBeNull()
    expect(screen.queryByText('Connection ID')).toBeNull()
    expect(screen.queryByText('No starting project selected')).toBeNull()
    expect(screen.queryByText('Ready in managed workspace')).toBeNull()
    expect(
      screen.queryByText(new RegExp(['workspace', 'project folder'].join(' '), 'i'))
    ).toBeNull()
    expect(screen.queryByText('claude')).toBeNull()
    expect(screen.queryByText('af-claude-container-123')).toBeNull()
    expect(screen.queryByText('/workspace')).toBeNull()
    expect(
      screen.getByText(
        /uses the saved tool selected in Settings\. Confirm where it opens project files before sending Tasks or code changes/i
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Saved tool')).toBeInTheDocument()
    expect(screen.queryByText('Work tool')).toBeNull()
    expect(screen.queryByText(/setup for its work tool/i)).toBeNull()
    expect(screen.queryByText(/file work/i)).toBeNull()
    expect(screen.queryByText(/before assigning work/i)).toBeNull()
    expect(screen.queryByText(/text-only model/i)).toBeNull()
    expect(screen.queryByText(/work profile/i)).toBeNull()
  })

  it('explains agents connected from this computer without exposing runtime ids', () => {
    render(<AgentConfigTab agentId="host1" />)

    expect(screen.getByTestId('agent-cli-config-summary')).toBeInTheDocument()
    expect(screen.getByText('This computer')).toBeInTheDocument()
    expect(screen.getByText('Connected from this computer')).toBeInTheDocument()
    expect(screen.getByText('Project for new tasks')).toBeInTheDocument()
    expect(screen.getByText('Platform')).toBeInTheDocument()
    expect(screen.getByText('Folder agents open')).toBeInTheDocument()
    expect(screen.getByText('Selected work folder')).toBeInTheDocument()
    expect(screen.queryByText('Starting project')).toBeNull()
    expect(screen.queryByText('Starting folder')).toBeNull()
    expect(screen.queryByText(/ran the command/i)).toBeNull()
    expect(screen.queryByText('host-local-123')).toBeNull()
    expect(screen.queryByText(/runtime/i)).toBeNull()
  })

  it('guides disconnected this-computer agents back to setup without command jargon', () => {
    render(<AgentConfigTab agentId="host-disconnected" />)

    expect(screen.getByText('This computer')).toBeInTheDocument()
    expect(screen.getByText('Open Agents and choose Connect this computer')).toBeInTheDocument()
    expect(screen.queryByText('Open setup again for this computer')).toBeNull()
    expect(screen.queryByText(/run the command/i)).toBeNull()
  })

  it('labels unknown saved tools without exposing raw tool values', () => {
    render(<AgentConfigTab agentId="future-tool" />)

    expect(screen.getByTestId('agent-cli-config-summary')).toBeInTheDocument()
    expect(screen.getByText('Check tool selected in Settings')).toBeInTheDocument()
    expect(screen.queryByText('Check work tool settings')).toBeNull()
    expect(screen.queryByText('future_tool')).toBeNull()
    expect(screen.queryByText('Unknown')).toBeNull()
  })

  it('tells users to refresh the saved tool setup when the CLI tool is missing', () => {
    render(<AgentConfigTab agentId="missing-tool" />)

    expect(screen.getByTestId('agent-cli-config-summary')).toBeInTheDocument()
    expect(screen.getByText('Check tool selected in Settings')).toBeInTheDocument()
    expect(screen.queryByText('Check work tool settings')).toBeNull()
    expect(screen.queryByText('Work tool not reported')).toBeNull()
  })

  it('shows a recovery step when the agent is no longer available', () => {
    render(<AgentConfigTab agentId="missing" />)
    expect(screen.getByText(/this agent could not be found/i)).toBeInTheDocument()
    expect(screen.getByText(/open agents, choose a current agent/i)).toBeInTheDocument()
    expect(screen.queryByText('Agent not found.')).toBeNull()
  })
})
