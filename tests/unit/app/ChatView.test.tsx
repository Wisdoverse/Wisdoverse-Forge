import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { ChatView } from '@app/features/chat/ChatView'
import { useAgentsStore, type AgentInfo } from '@app/entities/agent'
import { type Turn, useChatStore } from '@app/shared/model/chat.store'
import type { AgentMessageRow } from '@shared/types'

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

function message(content: string, overrides: Partial<AgentMessageRow> = {}): AgentMessageRow {
  return {
    id: overrides.id ?? 'msg-1',
    agentId: providerAgent.id,
    role: 'assistant' as const,
    content,
    tokensIn: null,
    tokensOut: null,
    model: providerAgent.model,
    finishReason: 'stop',
    createdAt: '2026-04-25T06:00:00Z',
    ...overrides,
  }
}

function turn(overrides: Partial<Turn>): Turn {
  return {
    id: 'turn-1',
    prompt: 'Investigate failing deploy',
    toolCalls: [],
    response: 'Deployment is ready',
    timestamp: Date.parse('2026-04-25T06:00:00Z'),
    ...overrides,
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
  const previousFindHelpCopy = new RegExp(['find', 'blockers'].join('\\s+'), 'i')
  const previousHandoffLabel = 'Conversation handoff'
  const previousHandoffTitle = 'Agent updates and help needed'

  test('shows chat-only AI service banner when agent has no cliTool', async () => {
    const loadMessages = vi.fn().mockResolvedValue(undefined)
    useAgentsStore.setState({ agents: [providerAgent] })
    seedChatState({ messages: [message('Hello from provider')], loadMessages })

    render(<ChatView agentId={providerAgent.id} />)

    const banner = screen.getByTestId('provider-agent-chat-banner')
    expect(banner).toBeInTheDocument()
    expect(within(banner).getByText('Chat-only AI service')).toBeInTheDocument()
    expect(within(banner).getByText(/messages use anthropic/i)).toBeInTheDocument()
    expect(
      within(banner).getByText(/can answer in chat.*does not work on workspace files/i)
    ).toBeInTheDocument()
    expect(banner).not.toHaveTextContent(/terminal/i)
    expect(banner).not.toHaveTextContent(/provider/i)
    expect(banner).not.toHaveTextContent(/model service/i)
    expect(banner).not.toHaveTextContent(/command window/i)
    expect(banner).not.toHaveTextContent('Chat-only agent')
    expect(screen.getByText('Hello from provider')).toBeInTheDocument()
    await waitFor(() => expect(loadMessages).toHaveBeenCalledWith(providerAgent.id))
  })

  test('does not expose raw AI service slugs in the chat-only banner', () => {
    useAgentsStore.setState({
      agents: [
        {
          ...providerAgent,
          id: 'unknown-provider-agent',
          provider: 'future_provider',
        },
      ],
    })
    seedChatState({ messages: [] })

    render(<ChatView agentId="unknown-provider-agent" />)

    const banner = screen.getByTestId('provider-agent-chat-banner')
    expect(within(banner).getByText(/messages use Check AI service/i)).toBeInTheDocument()
    expect(banner).not.toHaveTextContent(/future_provider/i)
    expect(banner).not.toHaveTextContent(/future provider/i)
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
    expect(screen.getByTestId('conversation-empty-state')).toBeInTheDocument()
    expect(screen.getByText('Start by asking this agent')).toBeInTheDocument()
    expect(screen.getByText('Ask for one outcome at a time.')).toBeInTheDocument()
    expect(screen.getByText('Use Attention after a reply to find what needs help.')).toBeVisible()
    expect(screen.getByText(/old messages are no longer useful/i)).toBeInTheDocument()
    expect(screen.queryByText(previousFindHelpCopy)).toBeNull()
    expect(screen.queryByText(/old context/i)).toBeNull()
  })

  test('guides empty managed workspace history toward routed work', () => {
    useAgentsStore.setState({ agents: [cliAgent] })
    seedChatState({ turns: [] })

    render(<ChatView agentId={cliAgent.id} />)

    expect(screen.getByTestId('conversation-empty-state')).toBeInTheDocument()
    expect(screen.getByText('Send work from Tasks to start updates')).toBeInTheDocument()
    expect(screen.getByText('Send work to create the first update.')).toBeInTheDocument()
    expect(
      screen.getByText(
        'Open Tasks and assign work to this agent or to a task queue it can receive.'
      )
    ).toBeInTheDocument()
    expect(
      screen.getByText('Check Attention once work starts to see what needs help.')
    ).toBeVisible()
    expect(screen.getByTestId('conversation-empty-state')).not.toHaveTextContent('lane')
    expect(screen.queryByText('No updates from this agent yet')).toBeNull()
    expect(screen.queryByText('No updates captured yet')).toBeNull()
    expect(screen.queryByText(previousFindHelpCopy)).toBeNull()
  })

  test('shows a clear retry path when workspace conversation history cannot load', () => {
    const fetchEvents = vi.fn().mockResolvedValue(undefined)
    useAgentsStore.setState({ agents: [cliAgent] })
    seedChatState({
      error:
        'Retry conversation to load conversation history. Check your connection, then choose Retry conversation again. Forge could not connect while loading this conversation.',
      fetchEvents,
    })

    render(<ChatView agentId={cliAgent.id} />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Conversation needs attention')
    expect(alert).toHaveTextContent('Check your connection, then choose Retry conversation again.')
    expect(alert).not.toHaveTextContent('HTTP')
    expect(alert).not.toHaveTextContent('Failed to fetch')

    fireEvent.click(screen.getByRole('button', { name: /retry conversation/i }))
    expect(fetchEvents).toHaveBeenCalledWith(cliAgent.id)
  })

  test('shows provider chat errors as attention without raw transport details', () => {
    useAgentsStore.setState({ agents: [providerAgent] })
    seedChatState({
      messages: [message('Earlier answer')],
      error:
        'Retry conversation to load conversation history. Forge could not load this conversation right now. Wait a few minutes, then try again. If it still fails, ask an owner or admin to check chat setup.',
    })

    render(<ChatView agentId={providerAgent.id} />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Conversation needs attention')
    expect(alert).toHaveTextContent('ask an owner or admin to check chat setup')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('Server error')
    expect(screen.getByText('Earlier answer')).toBeInTheDocument()
  })

  test('summarizes provider conversation handoff and filters updates', () => {
    useAgentsStore.setState({ agents: [providerAgent] })
    seedChatState({
      messages: [
        message('Please check the billing flow', {
          id: 'user-1',
          role: 'user',
          content: 'Please check the billing flow',
        }),
        message('Billing flow is blocked by a missing secret', {
          id: 'assistant-1',
          content: 'Billing flow is blocked by a missing secret',
          finishReason: 'error',
        }),
        message('Settings page shipped', {
          id: 'assistant-2',
          content: 'Settings page shipped',
          createdAt: '2026-04-25T06:10:00Z',
        }),
        message('Internal sender is not listed yet', {
          id: 'future-role',
          role: 'function_call' as never,
          content: 'Internal sender is not listed yet',
        }),
      ],
    })

    render(<ChatView agentId={providerAgent.id} />)

    expect(screen.getByTestId('conversation-handoff-summary')).toBeInTheDocument()
    expect(screen.getByText('Conversation summary')).toBeInTheDocument()
    expect(screen.getByText('Updates and next steps')).toBeInTheDocument()
    expect(screen.queryByText(previousHandoffLabel)).toBeNull()
    expect(screen.queryByText(previousHandoffTitle)).toBeNull()
    expect(
      screen.getByPlaceholderText('Search updates, help requests, work steps...')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-operator')).getByText('Your messages')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-operator')).getByText('1')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-agent')).getByText('2')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-attention')).getByText('1')
    ).toBeInTheDocument()
    expect(screen.getAllByText('You').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Agent').length).toBeGreaterThan(0)
    expect(screen.getByText('Check message sender')).toBeInTheDocument()
    expect(screen.queryByText('Message needs review')).toBeNull()
    expect(screen.queryByText(/^user$/i)).toBeNull()
    expect(screen.queryByText(/^assistant$/i)).toBeNull()
    expect(screen.queryByText(/function_call/i)).toBeNull()

    const filters = screen.getByTestId('conversation-filter-group')
    expect(within(filters).getByRole('button', { name: /you\s*1/i })).toBeInTheDocument()
    expect(within(filters).getByRole('button', { name: /work steps\s*0/i })).toBeInTheDocument()
    fireEvent.click(within(filters).getByRole('button', { name: /attention\s*1/i }))
    expect(screen.getByText('Billing flow is blocked by a missing secret')).toBeInTheDocument()
    expect(screen.queryByText('Settings page shipped')).toBeNull()

    fireEvent.change(screen.getByTestId('conversation-search'), {
      target: { value: 'missing-term' },
    })
    const emptyState = screen.getByTestId('conversation-filter-empty')
    expect(emptyState).toBeInTheDocument()
    expect(within(emptyState).getByText('Search and filter are hiding updates')).toBeInTheDocument()
    expect(within(emptyState).getByText(/useful updates may be hidden/i)).toBeInTheDocument()
    expect(within(emptyState).getByText(/review every update/i)).toBeInTheDocument()
    expect(emptyState).not.toHaveTextContent('No conversation updates match the current filters.')
    expect(emptyState).not.toHaveTextContent('Try All, Attention, or a shorter search term.')
    fireEvent.click(screen.getByRole('button', { name: /show all updates/i }))
    expect(screen.getByTestId('conversation-search')).toHaveValue('')
    expect(screen.getByText('Settings page shipped')).toBeInTheDocument()
  })

  test('summarizes CLI turns with tools and failed tool attention', async () => {
    const fetchEvents = vi.fn().mockResolvedValue(undefined)
    useAgentsStore.setState({ agents: [cliAgent] })
    seedChatState({
      fetchEvents,
      turns: [
        turn({
          id: 'turn-success',
          prompt: 'Run typecheck',
          toolCalls: [
            {
              toolUseId: 'tool-1',
              tool: 'shell',
              input: { command: 'npm run typecheck' },
              success: true,
            },
          ],
          response: 'Typecheck passed',
        }),
        turn({
          id: 'turn-failed',
          prompt: 'Deploy preview',
          toolCalls: [
            {
              toolUseId: 'tool-2',
              tool: 'deploy',
              input: { target: 'preview' },
              output: { error: 'Missing token' },
              success: false,
            },
          ],
          response: 'Deploy failed because credentials are missing',
        }),
      ],
    })

    render(<ChatView agentId={cliAgent.id} />)

    await waitFor(() => expect(fetchEvents).toHaveBeenCalledWith(cliAgent.id))
    expect(
      within(screen.getByTestId('conversation-metric-operator')).getByText('2')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-agent')).getByText('2')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-tools')).getByText('Work steps')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-tools')).getByText('2')
    ).toBeInTheDocument()
    expect(
      within(screen.getByTestId('conversation-metric-attention')).getByText('1')
    ).toBeInTheDocument()

    const filters = screen.getByTestId('conversation-filter-group')
    expect(within(filters).getByRole('button', { name: /work steps\s*2/i })).toBeInTheDocument()
    fireEvent.click(within(filters).getByRole('button', { name: /attention\s*1/i }))
    expect(screen.getByText(/Deploy failed because credentials are missing/i)).toBeInTheDocument()
    expect(screen.queryByText(/Typecheck passed/i)).toBeNull()
  })
})
