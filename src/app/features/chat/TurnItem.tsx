import { cn } from '@app/shared/lib/utils'
import type { Turn } from '@app/shared/model/chat.store'
import { ToolCallDetail } from './ToolCallDetail'

function formatTimestamp(ts: number): string {
  const d = new Date(ts)
  return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

export function TurnItem({ turn }: { turn: Turn }) {
  return (
    <div className="flex flex-col gap-2">
      {/* Timestamp */}
      <div className="text-[10px] text-secondary-light dark:text-secondary-dark text-center">
        {formatTimestamp(turn.timestamp)}
      </div>

      {/* User prompt */}
      {turn.prompt && (
        <div className="flex gap-2 items-start">
          <div
            className={cn(
              'min-w-10 h-6 rounded-full px-2 flex items-center justify-center shrink-0 mt-0.5',
              'bg-apple-blue/12 text-apple-blue text-[10px] font-semibold'
            )}
          >
            You
          </div>
          <div
            aria-label="Your message"
            className={cn(
              'flex-1 px-3 py-2 rounded-xl text-xs leading-relaxed',
              'bg-apple-blue/8 dark:bg-apple-blue/12',
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
          <p className="text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
            The agent recorded work steps during this turn. Open a step to see what happened before
            choosing the next move.
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
              'min-w-10 h-6 rounded-full px-2 flex items-center justify-center shrink-0 mt-0.5',
              'bg-black/5 dark:bg-white/10 text-secondary-light dark:text-secondary-dark',
              'text-[10px] font-semibold'
            )}
          >
            Agent
          </div>
          <div
            aria-label="Agent response"
            className={cn(
              'flex-1 px-3 py-2 rounded-xl text-xs leading-relaxed',
              'bg-black/[0.04] dark:bg-white/[0.06]',
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
