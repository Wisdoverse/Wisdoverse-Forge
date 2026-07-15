import { cn } from '@app/shared/lib/utils'
import type { Turn } from './model/chat.store'
import { ToolCallDetail } from './ToolCallDetail'

function formatTimestamp(ts: number): string {
  const d = new Date(ts)
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

export function TurnItem({ turn }: { turn: Turn }) {
  return (
    <div className="flex flex-col gap-2">
      {/* Timestamp */}
      <div className="text-center text-ui-caption text-secondary-light dark:text-secondary-dark">
        {formatTimestamp(turn.timestamp)}
      </div>

      {/* User prompt */}
      {turn.prompt && (
        <div className="flex gap-2 items-start">
          <div
            className={cn(
              'mt-0.5 flex h-6 min-w-10 shrink-0 items-center justify-center rounded-full border border-black/[0.08] px-2 text-ui-caption font-semibold text-secondary-light dark:border-white/[0.1] dark:text-secondary-dark'
            )}
          >
            You
          </div>
          <div
            aria-label="Your message"
            className={cn(
              'flex-1 rounded-card border border-black/[0.08] bg-black/[0.025] px-3 py-2 text-ui-caption leading-relaxed dark:border-white/[0.1] dark:bg-white/[0.05]',
              'text-foreground-light dark:text-foreground-dark'
            )}
          >
            {turn.prompt}
          </div>
        </div>
      )}

      {/* Work steps */}
      {turn.toolCalls.length > 0 && (
        <div aria-label="Work steps used by the agent" className="ml-12 flex flex-col gap-1.5">
          <p className="text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
            The agent saved step-by-step notes for this turn. Open a step to see what happened
            before choosing the next move.
          </p>
          {turn.toolCalls.map((call) => (
            <ToolCallDetail key={call.toolUseId} call={call} />
          ))}
        </div>
      )}

      {/* Assistant response */}
      {turn.response && (
        <div className="flex gap-2 items-start">
          <div
            className={cn(
              'mt-0.5 flex h-6 min-w-10 shrink-0 items-center justify-center rounded-full border border-black/[0.08] px-2 text-ui-caption font-semibold',
              'text-secondary-light dark:border-white/[0.1] dark:text-secondary-dark'
            )}
          >
            Agent
          </div>
          <div
            aria-label="Agent response"
            className={cn(
              'flex-1 rounded-card border border-black/[0.08] bg-white px-3 py-2 text-ui-caption leading-relaxed dark:border-white/[0.1] dark:bg-surface-dark',
              'text-foreground-light dark:text-foreground-dark'
            )}
          >
            {turn.response}
          </div>
        </div>
      )}
    </div>
  )
}
