import { useEffect, useRef } from 'react'
import { cn } from '@app/shared/lib/utils'
import { useChatStore } from '@app/shared/model/chat.store'
import { useAgentsStore } from '@app/shared/model/agents.store'
import { TurnItem } from './TurnItem'
import { ChatComposer } from './ChatComposer'
import { useChatStream } from './useChatStream'

interface ChatViewProps {
  agentId: string
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

  const currentLoading = isProviderAgent ? messagesLoading : loading

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
              <div className="text-sm text-secondary-light text-center">
                No conversation history yet
              </div>
            ) : (
              messages.map((m) => (
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
            <div className="text-sm text-secondary-light text-center">
              No conversation history yet
            </div>
          ) : (
            turns.map((turn) => <TurnItem key={turn.id} turn={turn} />)
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
        />
      )}
    </div>
  )
}
