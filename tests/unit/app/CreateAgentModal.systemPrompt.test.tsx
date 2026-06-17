import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'
import { CreateAgentModal } from '@app/features/agents/CreateAgentModal'
import { useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useSettingsStore } from '@app/shared/model/settings.store'

vi.mock('@app/entities/agent-group', () => ({
  agentGroupApi: { getGroups: vi.fn().mockResolvedValue([]) },
}))

afterEach(cleanup)

// A configured + tested provider so Provider + Prompt has a usable gateway
// option. Model is now derived from this configured provider, not a hardcoded list.
const CONFIGURED_PROVIDERS = [
  {
    id: 'provider-anthropic',
    provider: 'anthropic' as const,
    displayName: 'Anthropic',
    model: 'claude-sonnet-4-6',
    priority: 1,
    isEnabled: true,
    isDefault: true,
    lastTestStatus: 'passed' as const,
  },
]

beforeEach(() => {
  useAgentsStore.setState({
    createModalOpen: true,
    loading: false,
    error: null,
  })
  useSettingsStore.setState({
    providers: CONFIGURED_PROVIDERS,
    providersLoading: false,
    providersError: null,
  })
  useNavigationStore.setState({ selectedProjectId: null, projects: {} })
})

describe('CreateAgentModal agent instructions', () => {
  it('hides instruction textarea in CLI branch', () => {
    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /project files/i }))
    expect(screen.queryByLabelText(/agent instructions/i)).toBeNull()
  })

  it('defaults project-file work directory to /workspace', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /project files/i }))
    fireEvent.change(screen.getByPlaceholderText(/Frontend Agent/i), {
      target: { value: 'Test' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create agent/i }))

    await waitFor(() =>
      expect(createAgent).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: 'cli',
          cwd: '/workspace',
        })
      )
    )
  })

  it('shows instruction textarea when simple chat agent selected', () => {
    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    expect(screen.getByLabelText(/agent instructions/i)).toBeInTheDocument()
    expect(screen.queryByText(/system prompt/i)).toBeNull()
    expect(screen.queryByText(/prompt work/i)).toBeNull()
  })

  it('submits with lowercase provider + systemPrompt payload', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByPlaceholderText(/Frontend Agent/i), {
      target: { value: 'Test' },
    })
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.change(screen.getByLabelText(/agent instructions/i), {
      target: { value: 'you are terse' },
    })
    fireEvent.click(screen.getByRole('button', { name: /create agent/i }))
    await waitFor(() =>
      expect(createAgent).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: 'provider',
          provider: 'anthropic',
          model: 'claude-sonnet-4-6',
          systemPrompt: 'you are terse',
        })
      )
    )
  })

  it('omits systemPrompt when blank', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByPlaceholderText(/Frontend Agent/i), {
      target: { value: 'Test' },
    })
    fireEvent.click(screen.getByRole('radio', { name: /simple chat agent/i }))
    fireEvent.click(screen.getByRole('button', { name: /create agent/i }))
    await waitFor(() =>
      expect(createAgent).toHaveBeenCalledWith(
        expect.objectContaining({
          kind: 'provider',
          provider: 'anthropic',
          systemPrompt: undefined,
        })
      )
    )
  })
})
