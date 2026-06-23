import { FileText, ShieldAlert } from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { AppliedContextItem, TaskContextEvidence } from '@shared/types/context'

const HIDDEN_EVIDENCE_VALUE =
  'Hidden for safety. Reconnect the required account access, then retry.'
const MISSING_ACCESS_MESSAGE =
  'Required account access is missing. Add or reconnect service access, then retry.'
const TECHNICAL_EVIDENCE_MESSAGE =
  'Behind-the-scenes details were hidden for safety. Check the summary above, then ask the agent to explain what happened if the task still matters.'
const EMPTY_SAVED_DETAILS_MESSAGE =
  'The summary above is all that was saved for this item. Ask the agent to explain the result if something looks wrong.'
const MISSING_SAVED_DETAIL_VALUE = 'not saved for this item'

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
          What helped produce this result
        </h3>
        <p className="mt-0.5 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          These details show the answers, steps, and files used or saved so you can understand the
          result before taking the next step.
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
                  No longer used for future work. It stays here because this task already used it,
                  so you can still understand the past result.
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
                    Saved {formatRelativeTime(item.createdAt)}
                  </span>
                </div>
                <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
                  {evidenceDescription(item)}
                </p>
                <p className="mt-1 text-[10px] font-medium text-apple-blue">
                  {payloadSummary(item.payload)}
                </p>
                <p className="mt-1 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
                  Most users can rely on the summary above. Open saved details only when checking an
                  unexpected result or sharing details with an owner or admin.
                </p>
                <details className="mt-2 text-[10px] text-secondary-light dark:text-secondary-dark">
                  <summary className="cursor-pointer select-none font-medium text-foreground-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30 dark:text-foreground-dark">
                    Show saved details
                  </summary>
                  <pre className="mt-1 max-h-28 overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-white/70 p-2 leading-relaxed dark:bg-black/20">
                    {formatSavedDetails(item.payload)}
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
  if (item.sourceType === 'task_result') return 'Final answer'
  if (item.sourceType === 'tool_call') return 'Step the agent took'
  if (item.sourceType === 'artifact') return 'Saved result file'
  if (item.sourceType === 'source_message') return 'Message used for this work'
  return 'Work details'
}

function evidenceDescription(item: TaskContextEvidence): string {
  if (item.sourceType === 'task_result') {
    return "The agent's final answer or saved status for this task."
  }
  if (item.sourceType === 'tool_call') {
    return 'An action the agent took to complete the work.'
  }
  if (item.sourceType === 'artifact') {
    return 'A file or result saved while the work was running.'
  }
  if (item.sourceType === 'source_message') {
    return 'A message the agent used while preparing the result.'
  }
  return 'Extra information saved while the work was running.'
}

function payloadSummary(payload: Record<string, unknown>): string {
  const summary = firstString(payload.summary, payload.message, payload.title, payload.description)
  if (summary) return safeEvidenceString(summary)

  if (typeof payload.ok === 'boolean') {
    return payload.ok ? 'The saved result succeeded.' : 'Check the saved result before reusing it.'
  }

  const keys = Object.keys(payload)
  if (keys.length > 0) {
    return `Additional work details with ${keys.length} ${
      keys.length === 1 ? 'piece' : 'pieces'
    } of information.`
  }

  return 'Work details were saved for this task.'
}

function formatSavedDetails(payload: Record<string, unknown>): string {
  try {
    const lines = savedDetailLines(safeEvidenceValue(payload))
    return lines.length > 0 ? lines.join('\n') : EMPTY_SAVED_DETAILS_MESSAGE
  } catch {
    return 'Saved details could not be shown safely. Check the summary above, then ask an owner or admin to check this task if needed.'
  }
}

function savedDetailLines(value: unknown, label = 'Saved detail'): string[] {
  if (Array.isArray(value)) {
    if (value.length === 0) return [`${label}: Nothing else was saved.`]
    return value.flatMap((item, index) =>
      savedDetailLines(item, label === 'Saved detail' ? `Saved detail ${index + 1}` : label)
    )
  }

  if (value && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
    if (entries.length === 0) return [`${label}: Nothing else was saved.`]
    return entries.flatMap(([entryKey, entryValue]) =>
      savedDetailLines(entryValue, savedDetailLabel(entryKey))
    )
  }

  return [`${label}: ${savedDetailValue(value, label)}`]
}

function savedDetailLabel(key: string): string {
  if (isSensitiveEvidenceKey(key)) return 'Hidden detail'

  switch (key.trim().toLowerCase()) {
    case 'ok':
    case 'success':
      return 'Status'
    case 'summary':
      return 'Summary'
    case 'message':
      return 'Message'
    case 'title':
      return 'Title'
    case 'description':
      return 'Description'
    case 'error':
    case 'reason':
      return 'Problem'
    case 'retryable':
      return 'Can retry'
    default:
      return 'Saved detail'
  }
}

function savedDetailValue(value: unknown, label: string): string {
  if (typeof value === 'boolean') {
    if (label === 'Status') return value ? 'completed' : 'needs checking'
    return value ? 'yes' : 'no'
  }
  if (typeof value === 'number') return String(value)
  if (typeof value === 'string') return value
  if (value == null) return MISSING_SAVED_DETAIL_VALUE
  return 'saved but not shown here'
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
  const normalized = key.toLowerCase().replace(/[^a-z0-9]/g, '')
  return /(token|secret|password|apikey|privatekey|credential)/.test(normalized)
}

function safeEvidenceString(value: string): string {
  if (
    /\b(missing|invalid|expired)\s+(token|credential|credentials|api\s*key|secret)\b/i.test(value)
  ) {
    return MISSING_ACCESS_MESSAGE
  }
  if (containsSensitiveEvidenceText(value)) {
    return HIDDEN_EVIDENCE_VALUE
  }
  if (containsTechnicalEvidenceText(value)) {
    return TECHNICAL_EVIDENCE_MESSAGE
  }
  return value
}

function containsSensitiveEvidenceText(value: string): boolean {
  return /\b(authorization|bearer|secret\s+token|token\s+secret|access\s+token|refresh\s+token|api\s*key|private\s+key|password|credential)\b/i.test(
    value
  )
}

function containsTechnicalEvidenceText(value: string): boolean {
  return /\b((?:API|HTTP)\s*\d{3}|GraphQL|status code|provider|payload|endpoint|schema|panic|stack trace|traceback|exception|stdout|stderr|raw (?:command output|response|payload|event|details)|docker socket|internal error|database (?:unavailable|timeout|error)|connection refused)\b/i.test(
    value
  )
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim().length > 0) return value
  }
  return null
}
