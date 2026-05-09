import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { cleanup } from '@testing-library/react'
import { CreateAgentModal } from '@app/features/agents/CreateAgentModal'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { useNavigationStore } from '@app/entities/navigation'

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
  useNavigationStore.setState({ selectedProjectId: null, projects: {} })
})

describe('CreateAgentModal systemPrompt', () => {
  it('hides system prompt textarea in CLI branch', () => {
    render(<CreateAgentModal />)
    expect(screen.queryByLabelText(/system prompt/i)).toBeNull()
  })

  it('defaults CLI working directory to the container workspace', async () => {
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

  it('shows system prompt textarea when Provider + Prompt selected', () => {
    render(<CreateAgentModal />)
    fireEvent.click(screen.getByText(/Provider \+ Prompt/i))
    expect(screen.getByLabelText(/system prompt/i)).toBeInTheDocument()
  })

  it('submits with lowercase provider + systemPrompt payload', async () => {
    const createAgent = vi.fn().mockResolvedValue(true)
    useAgentsStore.setState({ createAgent } as never)

    render(<CreateAgentModal />)
    fireEvent.change(screen.getByPlaceholderText(/Frontend Agent/i), {
      target: { value: 'Test' },
    })
    fireEvent.click(screen.getByText(/Provider \+ Prompt/i))
    fireEvent.change(screen.getByLabelText(/system prompt/i), {
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
    fireEvent.click(screen.getByText(/Provider \+ Prompt/i))
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
