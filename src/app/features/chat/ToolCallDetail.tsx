import { useState } from 'react'
import { AlertTriangle, CheckCircle2, ChevronRight, Clock3, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import type { ToolCall } from '@app/shared/model/chat.store'

const MAX_OUTPUT_LINES = 12
const HIDDEN_ACCESS_VALUE = 'Hidden for safety. Reconnect the required account access, then retry.'
const SENSITIVE_ACCESS_MESSAGE =
  'Account access details were hidden. Reconnect the required account access, then retry if this step still matters.'
const MISSING_ACCESS_MESSAGE =
  'Required account access is missing. Add or reconnect service access, then retry.'
const TECHNICAL_PROBLEM_MESSAGE =
  'This step hit a problem. Ask the agent to explain what happened, then retry if the task still matters.'
const COMMAND_OUTPUT_MESSAGE =
  'The command result was saved. Ask the agent to explain it before relying on it.'
const PROBLEM_OUTPUT_MESSAGE =
  'The command problem details were saved. Ask the agent to explain what happened before retrying.'
const EMPTY_RESULT_SUMMARY =
  'This step finished, but it did not add details. Read the surrounding agent messages before deciding whether to continue, retry, or ask the agent to explain it.'
const EMPTY_RESULT_DETAILS =
  'No saved details were shown for this step. Read the surrounding agent messages before deciding whether to wait, retry, or ask the agent to explain it.'

function formatExtraDetails(data: Record<string, unknown>): string {
  try {
    const safeData = safeToolValue(data)
    if (!safeData || typeof safeData !== 'object' || Array.isArray(safeData)) {
      return formatExtraValue(safeData)
    }

    const lines = Object.entries(safeData as Record<string, unknown>).map(
      ([key, value]) => `${extraDetailLabel(key)}: ${formatExtraValue(value, key)}`
    )
    return lines.length > 0 ? lines.join('\n') : EMPTY_RESULT_DETAILS
  } catch {
    return 'Extra details were saved but could not be shown safely. Check the summary above, then ask an owner or admin to check this task if needed.'
  }
}

function extraDetailLabel(key: string): string {
  if (isSensitiveKey(key)) return 'Account access'

  const normalized = normalizedDetailKey(key)
  const labels: Record<string, string> = {
    command: 'Command',
    cwd: 'Where file work ran',
    durationms: 'Time spent',
    ok: 'Finished cleanly',
    summary: 'Summary',
    message: 'Message',
    description: 'Description',
    title: 'Title',
    query: 'Search text',
    path: 'File or link',
    file: 'File',
    url: 'Address',
    stdout: 'What the command showed',
    stderr: 'Problem details',
    rawoutput: 'What the command showed',
    commandoutput: 'What the command showed',
    erroroutput: 'Problem details',
    target: 'Target',
    reason: 'Reason',
    error: 'Problem',
  }
  return labels[normalized] ?? humanizeDetailKey(key)
}

function normalizedDetailKey(key: string): string {
  return key
    .trim()
    .toLowerCase()
    .replace(/[-_\s]/g, '')
}

function humanizeDetailKey(key: string): string {
  return key
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .split(/[_\-\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1).toLowerCase())
    .join(' ')
}

function formatExtraValue(value: unknown, key = ''): string {
  const normalized = normalizedDetailKey(key)
  if (typeof value === 'number' && normalized === 'durationms') {
    return formatDuration(value)
  }
  if (typeof value === 'string' && normalized === 'cwd') return formatProjectFolderValue(value)
  if (typeof value === 'boolean') return value ? 'Yes' : 'No'
  if (typeof value === 'string') return value
  if (value === null || value === undefined) return 'Not shown yet'
  if (typeof value === 'number') return String(value)
  return JSON.stringify(value, null, 2)
}

function formatProjectFolderValue(value: string): string {
  const folder = value.trim()
  if (!folder) return 'Project folder was not shown.'
  if (folder === '/workspace') {
    return 'Default agent project folder. You do not need to type this.'
  }
  return `${folder}. Use this only if an owner or admin asks where file work ran.`
}

function safeToolValue(value: unknown, key = ''): unknown {
  if (isSensitiveKey(key)) return HIDDEN_ACCESS_VALUE

  const savedOutputMessage = outputMessageForKey(key)
  if (savedOutputMessage) return savedOutputMessage

  if (typeof value === 'string') {
    return safeToolString(value)
  }

  if (Array.isArray(value)) {
    return value.map((item) => safeToolValue(item))
  }

  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([entryKey, entryValue]) => [
        entryKey,
        safeToolValue(entryValue, entryKey),
      ])
    )
  }

  return value
}

function outputMessageForKey(key: string): string | null {
  const normalized = normalizedDetailKey(key)
  if (['stdout', 'rawoutput', 'commandoutput'].includes(normalized)) {
    return COMMAND_OUTPUT_MESSAGE
  }
  if (['stderr', 'erroroutput'].includes(normalized)) {
    return PROBLEM_OUTPUT_MESSAGE
  }
  return null
}

function isSensitiveKey(key: string): boolean {
  const normalized = key.toLowerCase().replace(/[^a-z0-9]/g, '')
  return /(token|secret|password|apikey|credential)/.test(normalized)
}

function safeToolString(value: string): string {
  if (
    /\b(?:(?:missing|invalid|expired|revoked)\s+(?:token|credential|credentials|api\s*key|secret)|(?:token|credential|credentials|api\s*key|secret)\s+(?:missing|invalid|expired|revoked))\b/i.test(
      value
    )
  ) {
    return MISSING_ACCESS_MESSAGE
  }
  if (containsTechnicalProblemText(value)) {
    return TECHNICAL_PROBLEM_MESSAGE
  }
  if (containsSensitiveAccessText(value)) {
    return SENSITIVE_ACCESS_MESSAGE
  }
  return value
}

function containsSensitiveAccessText(value: string): boolean {
  return /\b(secret\s+token|token\s+secret|private\s+api\s*key|api\s*key\s+[\w.-]{4,}|password\s+[\w.-]{4,}|credential\s+[\w.-]{4,})\b/i.test(
    value
  )
}

function containsTechnicalProblemText(value: string): boolean {
  return /\b((?:API|HTTP)\s*\d{3}|status code|GraphQL|panic|stack trace|traceback|exception|stdout|stderr|raw command output|docker socket|internal error|database|connection refused)\b/i.test(
    value
  )
}

function formatDuration(duration: number): string {
  if (duration < 1000) return 'under 1 second'

  const seconds = Math.max(1, Math.round(duration / 1000))
  return `about ${seconds} ${seconds === 1 ? 'second' : 'seconds'}`
}

function toolDisplayName(tool: string): string {
  const normalized = tool.trim().toLowerCase()
  if (!normalized) return 'Work step'

  if (['shell', 'bash', 'terminal', 'command'].includes(normalized)) return 'Command step'
  if (['grep', 'ripgrep', 'search', 'web_search'].includes(normalized)) return 'Search'
  if (['read_file', 'file_read', 'open_file'].includes(normalized)) return 'Read file'
  if (['write_file', 'edit_file', 'apply_patch'].includes(normalized)) return 'Change files'
  if (['deploy', 'deployment'].includes(normalized)) return 'Publish step'

  return normalized
    .split(/[_\-\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

function toolDataSummary(data: Record<string, unknown>, kind: 'request' | 'result'): string {
  const directSummary = firstString(data.summary, data.message, data.title, data.description)
  if (directSummary) return safeToolString(directSummary)

  if (typeof data.command === 'string' && data.command.trim()) {
    return `Command the agent used: ${safeToolString(data.command)}`
  }

  if (typeof data.query === 'string' && data.query.trim()) {
    return `Search used: ${safeToolString(data.query)}`
  }

  const target = firstString(data.path, data.file, data.url)
  if (target) {
    return `Place checked: ${safeToolString(target)}`
  }

  const issue = firstString(data.error, data.reason)
  if (issue) {
    return `Check this step: ${safeToolString(issue)}`
  }

  if (typeof data.ok === 'boolean') {
    return data.ok
      ? 'This step finished successfully.'
      : 'Check this step before relying on the answer.'
  }

  const itemCount = Object.keys(data).length
  if (itemCount > 0) {
    return `${kind === 'request' ? 'Before this step' : 'After this step'} includes ${itemCount} ${itemCount === 1 ? 'item' : 'items'} you can inspect.`
  }

  return kind === 'request'
    ? 'The agent started this step without extra settings.'
    : EMPTY_RESULT_SUMMARY
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) return value.trim()
  }
  return null
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
      helper: 'This step finished without a problem.',
      tone: 'success',
      Icon: CheckCircle2,
    }
  }

  if (call.success === false) {
    return {
      label: 'Check step',
      helper: 'This step found a problem. Check it before trusting the answer.',
      tone: 'danger',
      Icon: AlertTriangle,
    }
  }

  return {
    label: 'Waiting for result',
    helper: 'The agent started this step. Wait for it to share what happened.',
    tone: 'pending',
    Icon: Clock3,
  }
}

export function toolCallSearchText(call: ToolCall): string {
  return [
    toolDisplayName(call.tool),
    toolOutcome(call).label,
    toolOutcome(call).helper,
    toolDataSummary(call.input, 'request'),
    formatExtraDetails(call.input),
    call.output ? toolDataSummary(call.output, 'result') : null,
    call.output ? formatExtraDetails(call.output) : null,
  ]
    .filter(Boolean)
    .join(' ')
    .toLowerCase()
}

export function ToolCallDetail({ call }: { call: ToolCall }) {
  const [expanded, setExpanded] = useState(false)
  const [showRequestDetails, setShowRequestDetails] = useState(false)
  const [showResultDetails, setShowResultDetails] = useState(false)
  const [showFullOutput, setShowFullOutput] = useState(false)

  const requestSummary = toolDataSummary(call.input, 'request')
  const requestDetails = formatExtraDetails(call.input)
  const outputSummary = call.output ? toolDataSummary(call.output, 'result') : null
  const outputText = call.output ? formatExtraDetails(call.output) : null
  const outputLines = outputText?.split('\n') ?? []
  const isTruncated = outputLines.length > MAX_OUTPUT_LINES
  const outcome = toolOutcome(call)
  const OutcomeIcon = outcome.Icon
  const readableTool = toolDisplayName(call.tool)

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
        aria-label={`${expanded ? 'Hide' : 'Show'} step details for ${readableTool}`}
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
            Agent saved a work step
          </p>
          <p className="truncate text-[10px] text-secondary-light dark:text-secondary-dark">
            Work step: {readableTool}. {outcome.helper}
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
            Finished in {formatDuration(call.duration)}
          </span>
        )}
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="px-3 pb-3 flex flex-col gap-2">
          <div className="rounded-md border border-black/[0.06] bg-white/60 px-3 py-2 text-[11px] leading-relaxed text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.03] dark:text-secondary-dark">
            This is a read-only summary of one step the agent took. Check the result, then decide
            whether to continue, retry, or ask the agent to explain it.
          </div>

          {call.success === false && (
            <div className="rounded-md border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-[11px] text-apple-red">
              Check this result before relying on the final answer.
            </div>
          )}

          {/* Request */}
          <div>
            <span className="text-[10px] font-medium text-secondary-light dark:text-secondary-dark uppercase tracking-wide">
              Before this step
            </span>
            <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
              What the agent was told or given before it ran this step.
            </p>
            <p className="mt-1 rounded-md bg-black/[0.035] px-3 py-2 text-[11px] leading-relaxed text-foreground-light dark:bg-white/[0.04] dark:text-foreground-dark">
              {requestSummary}
            </p>
            <button
              type="button"
              aria-expanded={showRequestDetails}
              onClick={() => setShowRequestDetails((visible) => !visible)}
              className="mt-1 text-[10px] font-medium text-apple-blue hover:underline"
            >
              {showRequestDetails ? 'Hide what the agent received' : 'Show what the agent received'}
            </button>
            {showRequestDetails && (
              <pre
                className={cn(
                  'mt-1 p-2 rounded-md text-[11px] leading-relaxed overflow-x-auto',
                  'bg-black/[0.04] dark:bg-white/[0.04]',
                  'text-foreground-light dark:text-foreground-dark',
                  'font-mono'
                )}
              >
                {requestDetails}
              </pre>
            )}
          </div>

          {/* Result */}
          {outputText ? (
            <div>
              <span className="text-[10px] font-medium text-secondary-light dark:text-secondary-dark uppercase tracking-wide">
                After this step
              </span>
              <p className="mt-0.5 text-[10px] text-secondary-light dark:text-secondary-dark">
                What the agent showed after this step finished.
              </p>
              <p className="mt-1 rounded-md bg-black/[0.035] px-3 py-2 text-[11px] leading-relaxed text-foreground-light dark:bg-white/[0.04] dark:text-foreground-dark">
                {outputSummary}
              </p>
              <button
                type="button"
                aria-expanded={showResultDetails}
                onClick={() => setShowResultDetails((visible) => !visible)}
                className="mt-1 text-[10px] font-medium text-apple-blue hover:underline"
              >
                {showResultDetails ? 'Hide what happened' : 'Show what happened'}
              </button>
              {showResultDetails && (
                <>
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
                      Show the rest of what happened ({outputLines.length} lines)
                    </button>
                  )}
                </>
              )}
            </div>
          ) : (
            <div className="rounded-md border border-black/[0.06] px-3 py-2 text-[11px] text-secondary-light dark:border-white/[0.08] dark:text-secondary-dark">
              This step does not have a result yet. Next: wait for another update before deciding
              whether to continue, retry, or ask the agent what is still running.
            </div>
          )}
        </div>
      )}
    </div>
  )
}
