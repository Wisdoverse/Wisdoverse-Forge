import { Lightbulb } from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { ContextCandidateKind, TaskContextCandidate } from '@shared/types/context'

interface ContextCandidatesListProps {
  title: string
  kind: ContextCandidateKind
  candidates: TaskContextCandidate[]
}

export function ContextCandidatesList({ title, kind, candidates }: ContextCandidatesListProps) {
  if (candidates.length === 0) return null

  return (
    <section className="space-y-2" data-testid={`context-candidates-${kind}`}>
      <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
        {title}
      </h3>
      <div className="space-y-2">
        {candidates.map((candidate) => (
          <article
            key={candidate.id}
            className="rounded-lg border border-black/[0.05] dark:border-white/[0.06] bg-apple-gray-6/70 dark:bg-white/[0.035] p-3"
          >
            <div className="flex items-start gap-2">
              <div className="mt-0.5 w-6 h-6 rounded-md bg-white dark:bg-white/[0.06] flex items-center justify-center text-apple-orange shrink-0">
                <Lightbulb size={14} strokeWidth={2} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <h4 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
                    {candidateTitle(candidate)}
                  </h4>
                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-badge bg-apple-orange/10 text-apple-orange">
                    {candidate.state}
                  </span>
                </div>
                <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark break-words">
                  {candidatePreview(candidate)}
                </p>
                <div className="mt-2 flex items-center justify-between gap-2 text-[10px] text-secondary-light dark:text-secondary-dark">
                  <span>Created {formatRelativeTime(candidate.createdAt)}</span>
                  {candidate.sourceRunId && <span>Run {candidate.sourceRunId.slice(0, 8)}</span>}
                </div>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}

function candidateTitle(candidate: TaskContextCandidate): string {
  const preview = candidate.proposedPreview
  for (const key of ['title', 'name', 'description']) {
    const value = preview[key]
    if (typeof value === 'string' && value.trim().length > 0) return value
  }
  return candidate.itemKind === 'skill' ? 'Skill candidate' : 'Memory update'
}

function candidatePreview(candidate: TaskContextCandidate): string {
  const value = candidate.proposedPreview.content_preview
  return typeof value === 'string' && value.trim().length > 0
    ? value
    : 'Candidate is waiting for review.'
}
