import { useState } from 'react'
import { AlertTriangle, CheckCircle2, ChevronRight, Clock3, type LucideIcon } from 'lucide-react'
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

function formatDuration(duration: number): string {
  return duration < 1000 ? `${duration}ms` : `${(duration / 1000).toFixed(1)}s`
}

function toolOutcome(call: ToolCall): {
  label: string
  helper: string
  tone: 'success' | 'danger' | 'pending'
  Icon: LucideIcon
} {
  if (call.success === true) {
    return {
      label: 'Completed cleanly',
      helper: 'The tool finished without reporting a problem.',
      tone: 'success',
      Icon: CheckCircle2,
    }
  }

  if (call.success === false) {
    return {
      label: 'Needs review',
      helper: 'The tool reported a problem. Check this before trusting the answer.',
      tone: 'danger',
      Icon: AlertTriangle,
    }
  }

  return {
    label: 'Waiting for result',
    helper: 'The agent started this step, but no result has been recorded yet.',
    tone: 'pending',
    Icon: Clock3,
  }
}

export function ToolCallDetail({ call }: { call: ToolCall }) {
  const [expanded, setExpanded] = useState(false)
  const [showFullOutput, setShowFullOutput] = useState(false)

  const outputText = call.output ? formatJson(call.output) : null
  const outputLines = outputText?.split('\n') ?? []
  const isTruncated = outputLines.length > MAX_OUTPUT_LINES
  const outcome = toolOutcome(call)
  const OutcomeIcon = outcome.Icon

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
        aria-expanded={expanded}
        aria-label={`${expanded ? 'Hide' : 'Show'} details for ${call.tool}`}
        className={cn(
          'w-full flex items-center gap-2 px-3 py-2 text-left',
          'hover:bg-black/[0.03] dark:hover:bg-white/[0.03]',
          'transition-colors rounded-lg'
        )}
      >
        <ChevronRight
          size={14}
          strokeWidth={2}
          aria-hidden="true"
          className={cn('shrink-0 transition-transform', expanded && 'rotate-90')}
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-foreground-light dark:text-foreground-dark">
            Agent used <code>{call.tool}</code>
          </p>
          <p className="truncate text-[10px] text-secondary-light dark:text-secondary-dark">
            {outcome.helper}
          </p>
        </div>
        <span
          className={cn(
            'inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium',
            outcome.tone === 'success' && 'bg-apple-green/12 text-apple-green',
            outcome.tone === 'danger' && 'bg-apple-red/12 text-apple-red',
            outcome.tone === 'pending' &&
              'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark'
          )}
        >
          <OutcomeIcon size={11} strokeWidth={2} aria-hidden="true" />
          {outcome.label}
        </span>
        {call.duration !== undefined && (
          <span className="hidden shrink-0 text-[10px] text-secondary-light dark:text-secondary-dark sm:inline">
            Took {formatDuration(call.duration)}
          </span>
        )}
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="px-3 pb-3 flex flex-col gap-2">
          {call.success === false && (
            <div className="rounded-md border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-[11px] text-apple-red">
              Review this result before relying on the final answer.
            </div>
          )}

          {/* Input */}
          <div>
            <span className="text-[10px] font-medium text-secondary-light dark:text-secondary-dark uppercase tracking-wide">
              What the agent sent
            </span>
            <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
              Settings or instructions passed into this step.
            </p>
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
          {outputText ? (
            <div>
              <span className="text-[10px] font-medium text-secondary-light dark:text-secondary-dark uppercase tracking-wide">
                What came back
              </span>
              <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
                Result returned by the tool.
              </p>
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
                  Show full result ({outputLines.length} lines)
                </button>
              )}
            </div>
          ) : (
            <div className="rounded-md border border-black/[0.06] px-3 py-2 text-[11px] text-secondary-light dark:border-white/[0.08] dark:text-secondary-dark">
              No result has been recorded for this step yet.
            </div>
          )}
        </div>
      )}
    </div>
  )
}
