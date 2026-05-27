import { useEffect, useMemo, useRef, useState } from 'react'
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  ListChecks,
  MessageCircle,
  Search,
  Terminal,
  UserRound,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { type Turn, useChatStore } from '@app/shared/model/chat.store'
import { useAgentsStore } from '@app/shared/model/agents.store'
import type { AgentMessageRow } from '@shared/types'
import { TurnItem } from './TurnItem'
import { ChatComposer } from './ChatComposer'
import { useChatStream } from './useChatStream'

interface ChatViewProps {
  agentId: string
}

type ConversationFilter = 'all' | 'operator' | 'agent' | 'tool' | 'attention'

const CONVERSATION_FILTERS: { value: ConversationFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'operator', label: 'Operator' },
  { value: 'agent', label: 'Agent' },
  { value: 'tool', label: 'Tool' },
  { value: 'attention', label: 'Attention' },
]

const PROVIDER_EMPTY_COPY = {
  title: 'Start by asking this agent',
  detail: 'Send a short request below when you need planning, review, or a direct answer.',
  steps: [
    'Ask for one outcome at a time.',
    'Use Attention after a reply to find blockers.',
    'Clear chat only when old context is no longer useful.',
  ],
}

const CLI_EMPTY_COPY = {
  title: 'No agent updates yet',
  detail:
    'Updates appear after this Container CLI agent receives work or reports terminal progress.',
  steps: [
    'Open Tasks and route work to this agent or its lane.',
    'Use Attention once work starts to find blockers.',
    'Refresh if the runtime just came online.',
  ],
}

export function ChatView({ agentId }: ChatViewProps) {
  const turns = useChatStore((s) => s.turns)
  const messages = useChatStore((s) => s.messages)
  const loading = useChatStore((s) => s.loading)
  const messagesLoading = useChatStore((s) => s.messagesLoading)
  const streaming = useChatStore((s) => s.streaming)
  const error = useChatStore((s) => s.error)
  const fetchEvents = useChatStore((s) => s.fetchEvents)
  const loadMessages = useChatStore((s) => s.loadMessages)
  const clearMessages = useChatStore((s) => s.clearMessages)
  const reset = useChatStore((s) => s.reset)

  const agent = useAgentsStore((s) => s.agents.find((a) => a.id === agentId))
  const isProviderAgent = agent != null && !agent.cliTool
  const offline = agent?.status === 'offline'
  const composerDisabledReason = offline
    ? 'This agent is offline. Start it before sending a message.'
    : messagesLoading
      ? 'Loading earlier messages. You can send once loading finishes.'
      : undefined
  const [conversationFilter, setConversationFilter] = useState<ConversationFilter>('all')
  const [conversationSearch, setConversationSearch] = useState('')
  const bottomRef = useRef<HTMLDivElement>(null)
  const { send, abort } = useChatStream(agentId)

  useEffect(() => {
    if (isProviderAgent) {
      void loadMessages(agentId)
    } else {
      void fetchEvents(agentId)
    }
    return () => reset()
  }, [agentId, isProviderAgent, loadMessages, fetchEvents, reset])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [turns, messages])

  useEffect(() => {
    setConversationFilter('all')
    setConversationSearch('')
  }, [agentId])

  const currentLoading = isProviderAgent ? messagesLoading : loading
  const transcriptStats = useMemo(
    () =>
      summarizeTranscript({
        isProviderAgent,
        messages,
        turns,
        offline,
        error,
      }),
    [error, isProviderAgent, messages, offline, turns]
  )
  const filterCounts = useMemo(
    () =>
      CONVERSATION_FILTERS.map((item) => ({
        ...item,
        count: isProviderAgent
          ? messages.filter((message) => messageMatchesFilter(message, item.value)).length
          : turns.filter((turn) => turnMatchesFilter(turn, item.value)).length,
      })),
    [isProviderAgent, messages, turns]
  )
  const visibleMessages = useMemo(
    () =>
      messages.filter((message) =>
        messageMatchesConversation(message, conversationFilter, conversationSearch)
      ),
    [conversationFilter, conversationSearch, messages]
  )
  const visibleTurns = useMemo(
    () =>
      turns.filter((turn) => turnMatchesConversation(turn, conversationFilter, conversationSearch)),
    [conversationFilter, conversationSearch, turns]
  )
  const hasActiveConversationFilter =
    conversationSearch.trim().length > 0 || conversationFilter !== 'all'

  function resetConversationFilters() {
    setConversationFilter('all')
    setConversationSearch('')
  }

  if (currentLoading && (isProviderAgent ? messages.length === 0 : turns.length === 0)) {
    return (
      <div
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-8',
          'shadow-card dark:shadow-card-dark',
          'flex items-center justify-center'
        )}
      >
        <div className="flex items-center gap-2 text-sm text-secondary-light dark:text-secondary-dark">
          <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none">
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
            />
          </svg>
          Loading conversation...
        </div>
      </div>
    )
  }

  if (error && !isProviderAgent) {
    return (
      <div
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl px-4 py-6',
          'shadow-card dark:shadow-card-dark',
          'flex flex-col items-center gap-3'
        )}
      >
        <span className="text-sm text-apple-red">{error}</span>
        <button
          type="button"
          onClick={() => void fetchEvents(agentId)}
          className={cn(
            'px-3 py-1.5 rounded-lg text-xs font-medium',
            'bg-apple-blue/10 text-apple-blue',
            'hover:bg-apple-blue/20 transition-colors'
          )}
        >
          Retry
        </button>
      </div>
    )
  }

  const banner =
    isProviderAgent && error ? (
      <div
        className={cn(
          'rounded-xl px-4 py-3 text-xs flex items-center justify-between',
          'bg-apple-red/10 text-apple-red border border-apple-red/20'
        )}
      >
        <span>{error}</span>
        {error.includes('context') && (
          <button type="button" onClick={() => void clearMessages(agentId)} className="underline">
            Clear chat
          </button>
        )}
      </div>
    ) : null

  const providerAgentBanner = isProviderAgent ? (
    <div
      data-testid="provider-agent-chat-banner"
      className={cn(
        'rounded-xl px-4 py-3 text-xs flex flex-col gap-1',
        'bg-apple-blue/10 text-apple-blue border border-apple-blue/20'
      )}
    >
      <span className="font-medium">Provider + Prompt agent</span>
      <span className="text-apple-blue/80">
        Messages are sent directly to {agent?.provider ?? 'the provider'} without a container
        terminal.
      </span>
    </div>
  ) : null

  return (
    <div className="flex flex-col gap-3">
      {banner}
      {providerAgentBanner}
      <section
        data-testid="conversation-handoff-summary"
        className="rounded-xl border border-black/[0.08] bg-white p-4 shadow-card dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:shadow-card-dark"
      >
        <div className="flex items-start gap-2">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
            <MessageCircle size={16} strokeWidth={2.15} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
              Conversation handoff
            </p>
            <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Agent updates and blockers
            </h3>
          </div>
        </div>

        <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <ConversationMetric
            testId="conversation-metric-operator"
            label="Operator"
            value={transcriptStats.operator}
            Icon={UserRound}
            tone="operator"
          />
          <ConversationMetric
            testId="conversation-metric-agent"
            label="Agent"
            value={transcriptStats.agent}
            Icon={Bot}
            tone="agent"
          />
          <ConversationMetric
            testId="conversation-metric-tools"
            label="Tools"
            value={transcriptStats.tools}
            Icon={Terminal}
            tone="tool"
          />
          <ConversationMetric
            testId="conversation-metric-attention"
            label="Attention"
            value={transcriptStats.attention}
            Icon={AlertTriangle}
            tone="attention"
          />
        </div>

        <p className="mt-3 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {transcriptStats.lastUpdate
            ? `Last update ${transcriptStats.lastUpdate}`
            : 'No updates captured yet'}
        </p>
      </section>

      <div className="flex flex-col gap-2">
        <label className="relative block">
          <span className="sr-only">Search conversation</span>
          <Search
            size={14}
            strokeWidth={2}
            className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-secondary-light dark:text-secondary-dark"
            aria-hidden="true"
          />
          <input
            data-testid="conversation-search"
            type="search"
            value={conversationSearch}
            onChange={(event) => setConversationSearch(event.target.value)}
            placeholder="Search updates, blockers, tools..."
            className={cn(
              'h-9 w-full rounded-lg border border-black/[0.08] bg-white pl-8 pr-3 text-ui-body outline-none',
              'text-foreground-light placeholder:text-secondary-light dark:border-white/[0.1] dark:bg-[#2c2c2e] dark:text-foreground-dark dark:placeholder:text-secondary-dark',
              'focus:ring-2 focus:ring-apple-blue-focus'
            )}
          />
        </label>
        <div
          role="group"
          aria-label="Conversation filter"
          data-testid="conversation-filter-group"
          className="inline-flex max-w-full items-center gap-1 overflow-x-auto rounded-lg bg-black/[0.035] p-1 dark:bg-white/[0.05]"
        >
          {filterCounts.map((item) => (
            <ConversationFilterButton
              key={item.value}
              active={conversationFilter === item.value}
              label={item.label}
              count={item.count}
              onClick={() => setConversationFilter(item.value)}
            />
          ))}
        </div>
      </div>

      <div
        className={cn(
          'bg-white dark:bg-[#2c2c2e] rounded-xl',
          'shadow-card dark:shadow-card-dark',
          'max-h-[60vh] overflow-y-auto'
        )}
      >
        <div className="flex flex-col gap-4 p-4">
          {isProviderAgent ? (
            messages.length === 0 ? (
              <ConversationEmptyState
                copy={PROVIDER_EMPTY_COPY}
                offline={offline}
                testId="conversation-empty-state"
              />
            ) : visibleMessages.length === 0 ? (
              <div
                data-testid="conversation-filter-empty"
                className="flex flex-col items-center gap-2 text-center text-sm text-secondary-light"
              >
                <span>No conversation updates match the current filters.</span>
                <span>Try All, Attention, or a shorter search term.</span>
                {hasActiveConversationFilter && (
                  <button
                    type="button"
                    onClick={resetConversationFilters}
                    className="rounded-full bg-apple-blue/10 px-3 py-1.5 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
                  >
                    Clear filters
                  </button>
                )}
              </div>
            ) : (
              visibleMessages.map((m) => (
                <div
                  key={m.id}
                  className={cn(
                    'flex flex-col gap-1',
                    m.role === 'user' ? 'items-end' : 'items-start'
                  )}
                >
                  <span className="text-[10px] text-secondary-light">{m.role}</span>
                  <div
                    className={cn(
                      'rounded-xl px-3 py-2 max-w-[80%] text-sm whitespace-pre-wrap',
                      m.role === 'user'
                        ? 'bg-apple-blue/10 text-apple-blue'
                        : 'bg-apple-gray-6 dark:bg-white/[0.06]'
                    )}
                  >
                    {m.content ||
                      (m.role === 'assistant' && streaming && m.finishReason == null ? '…' : '')}
                  </div>
                </div>
              ))
            )
          ) : turns.length === 0 ? (
            <ConversationEmptyState
              copy={CLI_EMPTY_COPY}
              offline={offline}
              testId="conversation-empty-state"
            />
          ) : visibleTurns.length === 0 ? (
            <div
              data-testid="conversation-filter-empty"
              className="flex flex-col items-center gap-2 text-center text-sm text-secondary-light"
            >
              <span>No conversation updates match the current filters.</span>
              <span>Try All, Attention, or a shorter search term.</span>
              {hasActiveConversationFilter && (
                <button
                  type="button"
                  onClick={resetConversationFilters}
                  className="rounded-full bg-apple-blue/10 px-3 py-1.5 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
                >
                  Clear filters
                </button>
              )}
            </div>
          ) : (
            visibleTurns.map((turn) => <TurnItem key={turn.id} turn={turn} />)
          )}
          <div ref={bottomRef} />
        </div>
      </div>
      {isProviderAgent && (
        <ChatComposer
          onSend={(content) => void send(content)}
          onAbort={abort}
          streaming={streaming}
          disabled={offline || messagesLoading || streaming}
          disabledReason={composerDisabledReason}
        />
      )}
    </div>
  )
}

function ConversationEmptyState({
  copy,
  offline,
  testId,
}: {
  copy: typeof PROVIDER_EMPTY_COPY
  offline: boolean
  testId: string
}) {
  return (
    <div data-testid={testId} className="mx-auto flex max-w-xl flex-col gap-3 py-4 text-left">
      <div className="flex items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
          <ListChecks size={17} strokeWidth={2.15} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            {copy.title}
          </p>
          <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
            {copy.detail}
          </p>
        </div>
      </div>
      <div className="grid gap-2 sm:grid-cols-3">
        {copy.steps.map((step) => (
          <div
            key={step}
            className="flex min-h-16 items-start gap-2 rounded-lg bg-black/[0.025] px-3 py-2 dark:bg-white/[0.05]"
          >
            <CheckCircle2
              size={14}
              strokeWidth={2.15}
              className="mt-0.5 shrink-0 text-apple-green"
              aria-hidden="true"
            />
            <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {step}
            </span>
          </div>
        ))}
      </div>
      {offline && (
        <p className="rounded-lg bg-apple-orange/10 px-3 py-2 text-ui-caption text-apple-orange">
          This agent is offline, so new updates will appear after the runtime is available.
        </p>
      )}
    </div>
  )
}

function ConversationMetric({
  testId,
  label,
  value,
  Icon,
  tone,
}: {
  testId: string
  label: string
  value: number
  Icon: LucideIcon
  tone: 'operator' | 'agent' | 'tool' | 'attention'
}) {
  const toneClass =
    tone === 'operator'
      ? 'text-apple-blue'
      : tone === 'agent'
        ? 'text-apple-green'
        : tone === 'attention'
          ? 'text-apple-orange'
          : 'text-secondary-light dark:text-secondary-dark'

  return (
    <div
      data-testid={testId}
      className="flex min-h-16 items-center gap-2 rounded-lg bg-black/[0.025] px-2.5 py-2 dark:bg-white/[0.05]"
    >
      <span
        className={cn(
          'flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-white dark:bg-black/20',
          toneClass
        )}
      >
        <Icon size={15} strokeWidth={2.2} aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
          {value}
        </span>
        <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
          {label}
        </span>
      </span>
    </div>
  )
}

function ConversationFilterButton({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean
  label: string
  count: number
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        'inline-flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-md px-2.5 text-ui-button font-medium transition-colors',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
        active
          ? 'bg-white text-foreground-light shadow-sm dark:bg-black/30 dark:text-foreground-dark'
          : 'text-secondary-light hover:bg-white/70 hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-black/20 dark:hover:text-foreground-dark'
      )}
    >
      <span>{label}</span>
      <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">{count}</span>
    </button>
  )
}

function summarizeTranscript({
  isProviderAgent,
  messages,
  turns,
  offline,
  error,
}: {
  isProviderAgent: boolean
  messages: AgentMessageRow[]
  turns: Turn[]
  offline: boolean
  error: string | null
}): {
  operator: number
  agent: number
  tools: number
  attention: number
  lastUpdate: string | null
} {
  if (isProviderAgent) {
    const lastMessage = messages.at(-1)
    return {
      operator: messages.filter((message) => message.role === 'user').length,
      agent: messages.filter((message) => message.role === 'assistant').length,
      tools: 0,
      attention:
        (offline ? 1 : 0) + (error ? 1 : 0) + messages.filter(messageNeedsAttention).length,
      lastUpdate: lastMessage ? formatMessageTime(lastMessage.createdAt) : null,
    }
  }

  const lastTurn = turns.at(-1)
  return {
    operator: turns.filter((turn) => Boolean(turn.prompt)).length,
    agent: turns.filter((turn) => Boolean(turn.response)).length,
    tools: turns.reduce((total, turn) => total + turn.toolCalls.length, 0),
    attention: (offline ? 1 : 0) + (error ? 1 : 0) + turns.filter(turnNeedsAttention).length,
    lastUpdate: lastTurn ? formatTurnTime(lastTurn.timestamp) : null,
  }
}

function messageMatchesConversation(
  message: AgentMessageRow,
  filter: ConversationFilter,
  query: string
): boolean {
  if (!messageMatchesFilter(message, filter)) return false
  const normalizedQuery = query.trim().toLowerCase()
  if (normalizedQuery.length === 0) return true
  return messageSearchText(message).includes(normalizedQuery)
}

function turnMatchesConversation(turn: Turn, filter: ConversationFilter, query: string): boolean {
  if (!turnMatchesFilter(turn, filter)) return false
  const normalizedQuery = query.trim().toLowerCase()
  if (normalizedQuery.length === 0) return true
  return turnSearchText(turn).includes(normalizedQuery)
}

function messageMatchesFilter(message: AgentMessageRow, filter: ConversationFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'operator') return message.role === 'user'
  if (filter === 'agent') return message.role === 'assistant'
  if (filter === 'tool') return false
  return messageNeedsAttention(message)
}

function turnMatchesFilter(turn: Turn, filter: ConversationFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'operator') return Boolean(turn.prompt)
  if (filter === 'agent') return Boolean(turn.response)
  if (filter === 'tool') return turn.toolCalls.length > 0
  return turnNeedsAttention(turn)
}

function messageNeedsAttention(message: AgentMessageRow): boolean {
  if (message.finishReason === 'error') return true
  return containsAttentionTerm(message.content)
}

function turnNeedsAttention(turn: Turn): boolean {
  return (
    turn.toolCalls.some((call) => call.success === false) ||
    containsAttentionTerm(turn.prompt) ||
    containsAttentionTerm(turn.response) ||
    turn.toolCalls.some(
      (call) => containsAttentionTerm(call.tool) || containsAttentionTerm(call.output)
    )
  )
}

function messageSearchText(message: AgentMessageRow): string {
  return [message.role, message.content, message.model, message.finishReason]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}

function turnSearchText(turn: Turn): string {
  return [
    turn.prompt,
    turn.response,
    ...turn.toolCalls.flatMap((call) => [
      call.tool,
      JSON.stringify(call.input),
      JSON.stringify(call.output),
    ]),
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}

function containsAttentionTerm(value: unknown): boolean {
  if (value == null) return false
  const text = typeof value === 'string' ? value : JSON.stringify(value)
  return /\b(blocked|blocker|failed|failure|error|denied|unauthorized|waiting|needs?)\b/i.test(text)
}

function formatMessageTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return 'recently'
  return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

function formatTurnTime(value: number): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return 'recently'
  return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}
