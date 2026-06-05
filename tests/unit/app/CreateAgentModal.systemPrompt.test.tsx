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

describe('CreateAgentModal systemPrompt', () => {
  it('hides instruction textarea in CLI branch', () => {
    render(<CreateAgentModal />)
    expect(screen.queryByLabelText(/instructions for this agent/i)).toBeNull()
  })

  it('defaults managed workspace work directory to /workspace', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByPlaceholderText(/Review work agent/i), {
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

  it('shows instruction textarea when Text-only model selected', () => {
    render(<CreateAgentModal />)
    fireEvent.click(screen.getByText(/Text-only model/i))
    expect(screen.getByLabelText(/instructions for this agent/i)).toBeInTheDocument()
    expect(screen.getByText(/tell the agent how to behave every time/i)).toBeInTheDocument()
  })

  it('submits with lowercase provider + systemPrompt payload', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByPlaceholderText(/Review work agent/i), {
      target: { value: 'Test' },
    })
    fireEvent.click(screen.getByText(/Text-only model/i))
    fireEvent.change(screen.getByLabelText(/instructions for this agent/i), {
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
    fireEvent.change(screen.getByPlaceholderText(/Review work agent/i), {
      target: { value: 'Test' },
    })
    fireEvent.click(screen.getByText(/Text-only model/i))
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
