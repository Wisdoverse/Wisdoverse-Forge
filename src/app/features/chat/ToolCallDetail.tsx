import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import type { ToolCall } from '@app/shared/model/chat.store'

const MAX_OUTPUT_LINES = 12

function formatJson(data: Record<string, unknown>): string {
  try {
    return JSON.stringify(data, null, 2)
  } catch {
    return String(data)
  }
}

export function ToolCallDetail({ call }: { call: ToolCall }) {
  const [expanded, setExpanded] = useState(false)
  const [showFullOutput, setShowFullOutput] = useState(false)

  const outputText = call.output ? formatJson(call.output) : null
  const outputLines = outputText?.split('\n') ?? []
  const isTruncated = outputLines.length > MAX_OUTPUT_LINES

  return (
    <div
      className={cn(
        'rounded-lg border',
        'border-black/5 dark:border-white/5',
        'bg-black/[0.02] dark:bg-white/[0.02]'
      )}
    >
      {/* Header */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className={cn(
          'w-full flex items-center gap-2 px-3 py-2 text-left',
          'hover:bg-black/[0.03] dark:hover:bg-white/[0.03]',
          'transition-colors rounded-lg'
        )}
      >
        <span className={cn('text-[10px] transition-transform', expanded ? 'rotate-90' : '')}>
          ▶
        </span>
        <code className="text-xs font-medium text-foreground-light dark:text-foreground-dark">
          {call.tool}
        </code>
        {call.success !== undefined && (
          <span
            className={cn(
              'text-[10px] px-1.5 py-0.5 rounded-full font-medium',
              call.success ? 'bg-apple-green/12 text-apple-green' : 'bg-apple-red/12 text-apple-red'
            )}
          >
            {call.success ? 'ok' : 'fail'}
          </span>
        )}
        {call.duration !== undefined && (
          <span className="text-[10px] text-secondary-light dark:text-secondary-dark ml-auto">
            {call.duration < 1000 ? `${call.duration}ms` : `${(call.duration / 1000).toFixed(1)}s`}
          </span>
        )}
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="px-3 pb-3 flex flex-col gap-2">
          {/* Input */}
          <div>
            <span className="text-[10px] font-medium text-secondary-light dark:text-secondary-dark uppercase tracking-wide">
              Input
            </span>
            <pre
              className={cn(
                'mt-1 p-2 rounded-md text-[11px] leading-relaxed overflow-x-auto',
                'bg-black/[0.04] dark:bg-white/[0.04]',
                'text-foreground-light dark:text-foreground-dark',
                'font-mono'
              )}
            >
              {formatJson(call.input)}
            </pre>
          </div>

          {/* Output */}
          {outputText && (
            <div>
              <span className="text-[10px] font-medium text-secondary-light dark:text-secondary-dark uppercase tracking-wide">
                Output
              </span>
              <pre
                className={cn(
                  'mt-1 p-2 rounded-md text-[11px] leading-relaxed overflow-x-auto',
                  'bg-black/[0.04] dark:bg-white/[0.04]',
                  'text-foreground-light dark:text-foreground-dark',
                  'font-mono'
                )}
              >
                {isTruncated && !showFullOutput
                  ? `${outputLines.slice(0, MAX_OUTPUT_LINES).join('\n')}\n...`
                  : outputText}
              </pre>
              {isTruncated && !showFullOutput && (
                <button
                  type="button"
                  onClick={() => setShowFullOutput(true)}
                  className="text-[10px] text-apple-blue hover:underline mt-1"
                >
                  Show more ({outputLines.length} lines)
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
