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
              'w-6 h-6 rounded-full flex items-center justify-center shrink-0 mt-0.5',
              'bg-apple-blue/12 text-apple-blue text-[10px] font-semibold'
            )}
          >
            U
          </div>
          <div
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

      {/* Tool calls */}
      {turn.toolCalls.length > 0 && (
        <div className="ml-8 flex flex-col gap-1.5">
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
              'w-6 h-6 rounded-full flex items-center justify-center shrink-0 mt-0.5',
              'bg-black/5 dark:bg-white/10 text-secondary-light dark:text-secondary-dark',
              'text-[10px] font-semibold'
            )}
          >
            A
          </div>
          <div
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
