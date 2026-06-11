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

beforeEach(() => {
  useAgentsStore.setState({
    createModalOpen: true,
    loading: false,
    error: null,
  })
  useSettingsStore.setState({
    providers: [],
    providersLoading: false,
    providersError: null,
  })
  useNavigationStore.setState({ selectedProjectId: null, projects: {} })
})

describe('CreateAgentModal agent instructions', () => {
  it('hides instruction textarea in CLI branch', () => {
    render(<CreateAgentModal />)
    expect(screen.queryByLabelText(/instructions for this agent/i)).toBeNull()
  })

  it('defaults managed workspace work directory to /workspace', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
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

  it('shows instruction textarea when Chat-only agent selected', () => {
    render(<CreateAgentModal />)
    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
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
    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
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
    fireEvent.click(screen.getByRole('radio', { name: /chat-only ai service/i }))
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
