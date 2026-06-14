import { FileText, ShieldAlert } from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { AppliedContextItem, TaskContextEvidence } from '@shared/types/context'

const HIDDEN_EVIDENCE_VALUE =
  'Hidden for safety. Reconnect the required account access, then retry.'
const MISSING_ACCESS_MESSAGE =
  'Required account access is missing. Add or reconnect service access, then retry.'

interface ContextEvidenceListProps {
  evidence: TaskContextEvidence[]
  revokedItems: AppliedContextItem[]
}

export function ContextEvidenceList({ evidence, revokedItems }: ContextEvidenceListProps) {
  if (evidence.length === 0 && revokedItems.length === 0) return null

  return (
    <section className="space-y-2" data-testid="context-evidence">
      <div>
        <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
          What the agent used
        </h3>
        <p className="mt-0.5 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          These records show what the agent used or saved so you can understand the result before
          taking the next step.
        </p>
      </div>
      <div className="space-y-2">
        {revokedItems.map((item) => (
          <article
            key={`revoked-${item.injectionId}`}
            className="rounded-lg border border-apple-red/25 bg-apple-red/[0.04] p-3"
          >
            <div className="flex items-start gap-2">
              <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-white text-apple-red dark:bg-white/[0.06]">
                <ShieldAlert size={14} strokeWidth={2} aria-hidden="true" />
              </div>
              <div className="min-w-0 flex-1">
                <p className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
                  {item.title}
                </p>
                <p className="mt-1 text-[11px] leading-relaxed text-apple-red">
                  No longer used for future work. It stays here because this run already used it, so
                  you can still understand the past result.
                </p>
              </div>
            </div>
          </article>
        ))}
        {evidence.map((item) => (
          <article
            key={`${item.sourceType}-${item.sourceId}`}
            className="rounded-lg border border-black/[0.05] dark:border-white/[0.06] bg-apple-gray-6/70 dark:bg-white/[0.035] p-3"
          >
            <div className="flex items-start gap-2">
              <div className="mt-0.5 w-6 h-6 rounded-md bg-white dark:bg-white/[0.06] flex items-center justify-center text-apple-blue shrink-0">
                <FileText size={14} strokeWidth={2} aria-hidden="true" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <p className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
                    {evidenceTitle(item)}
                  </p>
                  <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
                    Recorded {formatRelativeTime(item.createdAt)}
                  </span>
                </div>
                <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
                  {evidenceDescription(item)}
                </p>
                <p className="mt-1 text-[10px] font-medium text-apple-blue">
                  {payloadSummary(item.payload)}
                </p>
                <p className="mt-1 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
                  Most users can rely on the summary above. Open support details only when checking
                  an unexpected result or sharing details with support.
                </p>
                <details className="mt-2 text-[10px] text-secondary-light dark:text-secondary-dark">
                  <summary className="cursor-pointer select-none font-medium text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30 dark:text-foreground-dark">
                    Show support details
                  </summary>
                  <pre className="mt-1 max-h-28 overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-white/70 p-2 leading-relaxed dark:bg-black/20">
                    {formatTechnicalDetails(item.payload)}
                  </pre>
                </details>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}

function evidenceTitle(item: TaskContextEvidence): string {
  if (item.sourceType === 'task_result') return 'Task result'
  if (item.sourceType === 'tool_call') return 'Tool activity'
  if (item.sourceType === 'artifact') return 'Saved result file'
  if (item.sourceType === 'source_message') return 'Source message'
  return 'Work details'
}

function evidenceDescription(item: TaskContextEvidence): string {
  if (item.sourceType === 'task_result') {
    return 'Final answer or status saved from the agent work.'
  }
  if (item.sourceType === 'tool_call') {
    return 'A recorded tool action that helped the agent complete the work.'
  }
  if (item.sourceType === 'artifact') {
    return 'A file or result saved while the task ran.'
  }
  if (item.sourceType === 'source_message') {
    return 'A message the agent used while preparing the result.'
  }
  return 'Extra information recorded while the task ran.'
}

function payloadSummary(payload: Record<string, unknown>): string {
  const summary = firstString(payload.summary, payload.message, payload.title, payload.description)
  if (summary) return summary

  if (typeof payload.ok === 'boolean') {
    return payload.ok ? 'The recorded result succeeded.' : 'The recorded result needs attention.'
  }

  const keys = Object.keys(payload)
  if (keys.length > 0) {
    return `Additional work details with ${keys.length} ${
      keys.length === 1 ? 'piece' : 'pieces'
    } of information.`
  }

  return 'Work details were recorded for this task.'
}

function formatTechnicalDetails(payload: Record<string, unknown>): string {
  try {
    return JSON.stringify(safeEvidenceValue(payload), null, 2)
  } catch {
    return 'Details for support were recorded but could not be shown safely. Review the summary above, then ask support to check this task if needed.'
  }
}

function safeEvidenceValue(value: unknown, key = ''): unknown {
  if (isSensitiveEvidenceKey(key)) return HIDDEN_EVIDENCE_VALUE

  if (typeof value === 'string') return safeEvidenceString(value)

  if (Array.isArray(value)) return value.map((item) => safeEvidenceValue(item))

  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([entryKey, entryValue]) => [
        entryKey,
        safeEvidenceValue(entryValue, entryKey),
      ])
    )
  }

  return value
}

function isSensitiveEvidenceKey(key: string): boolean {
  return /\b(token|secret|password|api[_-]?key|credential|credentials)\b/i.test(key)
}

function safeEvidenceString(value: string): string {
  if (
    /\b(missing|invalid|expired)\s+(token|credential|credentials|api\s*key|secret)\b/i.test(value)
  ) {
    return MISSING_ACCESS_MESSAGE
  }
  return value
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) return value
  }
  return null
}
