import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { ChatView } from '@app/features/chat/ChatView'
import { useAgentsStore, type AgentInfo } from '@app/shared/model/agents.store'
import { useChatStore } from '@app/shared/model/chat.store'

const providerAgent: AgentInfo = {
  id: 'provider-agent',
  name: 'Provider Agent',
  provider: 'Anthropic',
  model: 'claude-sonnet-4-6',
  status: 'idle',
  tasksCompleted: 0,
  tasksInProgress: 0,
  successRate: 0,
  cliTool: undefined,
}

const cliAgent: AgentInfo = {
  ...providerAgent,
  id: 'cli-agent',
  name: 'CLI Agent',
  cliTool: 'claude',
  containerId: 'container-1',
}

function message(content: string) {
  return {
    id: 'msg-1',
    agentId: providerAgent.id,
    role: 'assistant' as const,
    content,
    tokensIn: null,
    tokensOut: null,
    model: providerAgent.model,
    finishReason: 'stop',
    createdAt: '2026-04-25T06:00:00Z',
  }
}

function seedChatState(overrides: Partial<ReturnType<typeof useChatStore.getState>> = {}) {
  useChatStore.setState({
    turns: [],
    messages: [],
    loading: false,
    messagesLoading: false,
    streaming: false,
    streamingMessageId: null,
    error: null,
    fetchEvents: vi.fn().mockResolvedValue(undefined),
    loadMessages: vi.fn().mockResolvedValue(undefined),
    clearMessages: vi.fn().mockResolvedValue(undefined),
    reset: vi.fn(),
    ...overrides,
  })
}

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn()
  useAgentsStore.getState().reset()
  seedChatState()
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('ChatView', () => {
  test('shows provider-agent banner when agent has no cliTool', async () => {
    const loadMessages = vi.fn().mockResolvedValue(undefined)
    useAgentsStore.setState({ agents: [providerAgent] })
    seedChatState({ messages: [message('Hello from provider')], loadMessages })

    render(<ChatView agentId={providerAgent.id} />)

    expect(screen.getByTestId('provider-agent-chat-banner')).toBeInTheDocument()
    expect(screen.getByText('Hello from provider')).toBeInTheDocument()
    await waitFor(() => expect(loadMessages).toHaveBeenCalledWith(providerAgent.id))
  })

  test('does not show banner for container CLI agent', async () => {
    const fetchEvents = vi.fn().mockResolvedValue(undefined)
    useAgentsStore.setState({ agents: [cliAgent] })
    seedChatState({ fetchEvents })

    render(<ChatView agentId={cliAgent.id} />)

    expect(screen.queryByTestId('provider-agent-chat-banner')).toBeNull()
    await waitFor(() => expect(fetchEvents).toHaveBeenCalledWith(cliAgent.id))
  })

  test('banner still renders with empty-state history', () => {
    useAgentsStore.setState({ agents: [providerAgent] })
    seedChatState({ messages: [] })

    render(<ChatView agentId={providerAgent.id} />)

    expect(screen.getByTestId('provider-agent-chat-banner')).toBeInTheDocument()
    expect(screen.getByText('No conversation history yet')).toBeInTheDocument()
  })
})
