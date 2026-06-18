import { useEffect, useMemo, useRef, useState } from 'react'
import {
  AlertTriangle,
  ArrowRight,
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
import { agentAiServiceLabel, useAgentsStore } from '@app/entities/agent'
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
  { value: 'operator', label: 'You' },
  { value: 'agent', label: 'Agent' },
  { value: 'tool', label: 'Work steps' },
  { value: 'attention', label: 'Attention' },
]

const PROVIDER_EMPTY_COPY = {
  title: 'Start by asking this agent',
  detail: 'Send a short request below when you need planning, review, or a direct answer.',
  steps: [
    'Ask for one outcome at a time.',
    'Use Attention after a reply to find what needs help.',
    'Clear chat only when old messages are no longer useful.',
  ],
}

const WORKSPACE_AGENT_EMPTY_COPY = {
  title: 'Send this agent a task to start updates',
  detail: 'This history fills in after the agent receives work or reports progress.',
  steps: [
    'Create a task and assign it to this agent, or choose where tasks wait so this agent can receive it.',
    'Check Attention once work starts to see what needs help.',
    'Refresh if the agent just came online.',
  ],
}

interface ConversationFilterEmptyCopy {
  title: string
  detail: string
  nextStep: string
}

interface ConversationEmptyAction {
  label: string
  href: string
}

function conversationFilterEmptyCopy(
  filter: ConversationFilter,
  search: string
): ConversationFilterEmptyCopy {
  const hasSearch = search.trim().length > 0
  const filterLabel =
    CONVERSATION_FILTERS.find((item) => item.value === filter)?.label ?? 'selected'

  if (hasSearch && filter !== 'all') {
    return {
      title: 'Search and filter are hiding updates',
      detail:
        'The search is only looking inside the selected view, so useful updates may be hidden.',
      nextStep: 'Next: clear filters, review every update, then search again with one short word.',
    }
  }

  if (hasSearch) {
    return {
      title: 'Search did not find a conversation update',
      detail: 'Try one word from the update, such as the task name, result, or help request.',
      nextStep: 'Next: clear the search to see every update again.',
    }
  }

  if (filter === 'attention') {
    return {
      title: 'Use All if you expected a blocker',
      detail: 'No message is stuck, failed, waiting, or asking for your help in this view.',
      nextStep:
        'Next: use All to read the full conversation, or send a short follow-up if you expected a blocker.',
    }
  }

  if (filter === 'operator') {
    return {
      title: 'Send a message to see your requests here',
      detail: 'The You filter only shows requests you sent.',
      nextStep: 'Next: use All to review every update, or send a message below to add a request.',
    }
  }

  if (filter === 'agent') {
    return {
      title: 'Wait for the agent reply, or use All',
      detail: 'The Agent filter only shows answers or progress notes from the agent.',
      nextStep: 'Next: use All to see the full history, or wait for the agent to report progress.',
    }
  }

  if (filter === 'tool') {
    return {
      title: 'Assign a task to see work steps',
      detail: 'Work steps appear when an agent shares commands or tool results.',
      nextStep: 'Next: use All to see chat updates, or assign a task so work steps can appear.',
    }
  }

  return {
    title: `No updates in ${filterLabel} yet`,
    detail: 'This view has no matching conversation updates right now.',
    nextStep: 'Next: use All to see every update.',
  }
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
  const offlineRecoveryDetail = isProviderAgent
    ? 'This chat-only AI service is not ready. Open AI service settings, choose Check connection, then refresh Agents.'
    : 'This agent is not ready. Open Agents, start or reconnect it, then return here when it shows Ready.'
  const emptyAction: ConversationEmptyAction | undefined = isProviderAgent
    ? offline
      ? { label: 'Open AI services', href: '/settings/providers' }
      : undefined
    : offline
      ? { label: 'Open Agents', href: '/agents' }
      : { label: 'Create a task', href: '/tasks' }
  const composerDisabledReason = offline
    ? 'Open AI service settings, choose Check connection, then refresh Agents before sending a message.'
    : messagesLoading
      ? 'Wait for earlier messages to finish loading, then send your message from this chat.'
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
          Loading updates...
        </div>
      </div>
    )
  }

  if (error && !isProviderAgent) {
    return (
      <ChatErrorNotice
        message={error}
        actionLabel="Retry conversation"
        onAction={() => void fetchEvents(agentId)}
      />
    )
  }

  const banner =
    isProviderAgent && error ? (
      <ChatErrorNotice
        message={error}
        actionLabel={error.includes('context') ? 'Clear chat' : undefined}
        onAction={error.includes('context') ? () => void clearMessages(agentId) : undefined}
      />
    ) : null

  const modelServiceName = agent ? agentAiServiceLabel(agent.provider) : 'your saved AI service'
  const providerAgentBanner = isProviderAgent ? (
    <div
      data-testid="provider-agent-chat-banner"
      className={cn(
        'rounded-xl px-4 py-3 text-xs flex flex-col gap-1',
        'bg-apple-blue/10 text-apple-blue border border-apple-blue/20'
      )}
    >
      <span className="font-medium">Chat-only AI service</span>
      <span className="text-apple-blue/80">
        Messages use {modelServiceName}. This agent can answer in chat, but it does not open project
        files.
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
              Conversation summary
            </p>
            <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
              Updates and next steps
            </h3>
          </div>
        </div>

        <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-4">
          <ConversationMetric
            testId="conversation-metric-operator"
            label="Your messages"
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
            label="Work steps"
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
            : 'Send work to create the first update.'}
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
            placeholder="Search updates, help requests, work steps..."
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
                offlineDetail={offlineRecoveryDetail}
                action={emptyAction}
                testId="conversation-empty-state"
              />
            ) : visibleMessages.length === 0 ? (
              <ConversationFilterEmptyState
                filter={conversationFilter}
                search={conversationSearch}
                onClear={resetConversationFilters}
              />
            ) : (
              visibleMessages.map((m) => {
                const role = messageRoleKey(m.role)
                return (
                  <div
                    key={m.id}
                    className={cn(
                      'flex flex-col gap-1',
                      role === 'user' ? 'items-end' : 'items-start'
                    )}
                  >
                    <span className="text-[10px] text-secondary-light">
                      {messageRoleLabel(m.role)}
                    </span>
                    <div
                      className={cn(
                        'rounded-xl px-3 py-2 max-w-[80%] text-sm whitespace-pre-wrap',
                        role === 'user'
                          ? 'bg-apple-blue/10 text-apple-blue'
                          : 'bg-apple-gray-6 dark:bg-white/[0.06]'
                      )}
                    >
                      {m.content ||
                        (role === 'assistant' && streaming && m.finishReason == null ? '…' : '')}
                    </div>
                  </div>
                )
              })
            )
          ) : turns.length === 0 ? (
            <ConversationEmptyState
              copy={WORKSPACE_AGENT_EMPTY_COPY}
              offline={offline}
              offlineDetail={offlineRecoveryDetail}
              action={emptyAction}
              testId="conversation-empty-state"
            />
          ) : visibleTurns.length === 0 ? (
            <ConversationFilterEmptyState
              filter={conversationFilter}
              search={conversationSearch}
              onClear={resetConversationFilters}
            />
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

function ConversationFilterEmptyState({
  filter,
  search,
  onClear,
}: {
  filter: ConversationFilter
  search: string
  onClear: () => void
}) {
  const copy = conversationFilterEmptyCopy(filter, search)
  return (
    <div
      data-testid="conversation-filter-empty"
      className="flex flex-col items-center gap-2 text-center text-sm text-secondary-light"
    >
      <span className="font-medium text-foreground-light dark:text-foreground-dark">
        {copy.title}
      </span>
      <span>{copy.detail}</span>
      <span className="text-ui-caption">{copy.nextStep}</span>
      <button
        type="button"
        onClick={onClear}
        className="rounded-full bg-apple-blue/10 px-3 py-1.5 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
      >
        Show all updates
      </button>
    </div>
  )
}

function ChatErrorNotice({
  message,
  actionLabel,
  onAction,
}: {
  message: string
  actionLabel?: string
  onAction?: () => void
}) {
  return (
    <div
      role="alert"
      aria-live="polite"
      className={cn(
        'rounded-xl border border-apple-red/20 bg-apple-red/10 px-4 py-3 text-left text-apple-red',
        'flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between'
      )}
    >
      <span className="flex min-w-0 items-start gap-2">
        <AlertTriangle
          size={15}
          strokeWidth={2.25}
          className="mt-0.5 shrink-0"
          aria-hidden="true"
        />
        <span className="min-w-0">
          <span className="block text-ui-caption font-semibold">Check this conversation</span>
          <span className="mt-0.5 block text-ui-caption">{message}</span>
        </span>
      </span>
      {actionLabel && onAction && (
        <button
          type="button"
          onClick={onAction}
          className="shrink-0 rounded-full bg-white/70 px-3 py-1.5 text-ui-button font-medium text-apple-red transition-colors hover:bg-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30 dark:bg-black/20 dark:hover:bg-black/30"
        >
          {actionLabel}
        </button>
      )}
    </div>
  )
}

function ConversationEmptyState({
  copy,
  offline,
  offlineDetail,
  action,
  testId,
}: {
  copy: typeof PROVIDER_EMPTY_COPY
  offline: boolean
  offlineDetail: string
  action?: ConversationEmptyAction
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
          {offlineDetail}
        </p>
      )}
      {action && (
        <a
          data-testid="conversation-empty-action"
          href={action.href}
          className="inline-flex h-9 w-fit items-center gap-1.5 rounded-full bg-apple-blue px-3 text-ui-button font-medium text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
        >
          <span>{action.label}</span>
          <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
        </a>
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
  const role = messageRoleKey(message.role)
  if (filter === 'all') return true
  if (filter === 'operator') return role === 'user'
  if (filter === 'agent') return role === 'assistant'
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
  return [messageRoleLabel(message.role), message.content, message.model, message.finishReason]
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

function messageRoleKey(role: string): string {
  return role.trim().toLowerCase()
}

function messageRoleLabel(role: string): string {
  switch (messageRoleKey(role)) {
    case 'user':
      return 'You'
    case 'assistant':
      return 'Agent'
    default:
      return role.trim() ? 'Check message sender' : 'Refresh chat to load sender'
  }
}

function formatTurnTime(value: number): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return 'recently'
  return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}
