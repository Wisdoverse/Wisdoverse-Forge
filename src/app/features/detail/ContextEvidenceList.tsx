import { FileText } from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { AppliedContextItem, TaskContextEvidence } from '@shared/types/context'

interface ContextEvidenceListProps {
  evidence: TaskContextEvidence[]
  revokedItems: AppliedContextItem[]
}

export function ContextEvidenceList({ evidence, revokedItems }: ContextEvidenceListProps) {
  if (evidence.length === 0 && revokedItems.length === 0) return null

  return (
    <section className="space-y-2" data-testid="context-evidence">
      <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
        Evidence
      </h3>
      <div className="space-y-2">
        {revokedItems.map((item) => (
          <article
            key={`revoked-${item.injectionId}`}
            className="rounded-lg border border-apple-red/25 bg-apple-red/[0.04] p-3"
          >
            <p className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
              {item.title}
            </p>
            <p className="mt-1 text-[11px] text-apple-red">
              Revoked context remains visible because it was applied to this run.
            </p>
          </article>
        ))}
        {evidence.map((item) => (
          <article
            key={`${item.sourceType}-${item.sourceId}`}
            className="rounded-lg border border-black/[0.05] dark:border-white/[0.06] bg-apple-gray-6/70 dark:bg-white/[0.035] p-3"
          >
            <div className="flex items-start gap-2">
              <div className="mt-0.5 w-6 h-6 rounded-md bg-white dark:bg-white/[0.06] flex items-center justify-center text-apple-blue shrink-0">
                <FileText size={14} strokeWidth={2} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-2">
                  <p className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
                    {item.sourceType}
                  </p>
                  <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
                    {formatRelativeTime(item.createdAt)}
                  </span>
                </div>
                <pre className="mt-1 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark whitespace-pre-wrap break-words max-h-28 overflow-y-auto">
                  {JSON.stringify(item.payload, null, 2)}
                </pre>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}
