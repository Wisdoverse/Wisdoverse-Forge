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
      <div>
        <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </h3>
        <p className="mt-0.5 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          {sectionDescription(kind)}
        </p>
      </div>
      <div className="space-y-2">
        {candidates.map((candidate) => (
          <article
            key={candidate.id}
            className="rounded-lg border border-black/[0.05] dark:border-white/[0.06] bg-apple-gray-6/70 dark:bg-white/[0.035] p-3"
          >
            <div className="flex items-start gap-2">
              <div className="mt-0.5 w-6 h-6 rounded-md bg-white dark:bg-white/[0.06] flex items-center justify-center text-apple-orange shrink-0">
                <Lightbulb size={14} strokeWidth={2} aria-hidden="true" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <h4 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
                    {candidateTitle(candidate)}
                  </h4>
                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-badge bg-apple-blue/10 text-apple-blue">
                    {candidateKindLabel(candidate)}
                  </span>
                  <span className="text-[10px] font-medium px-1.5 py-0.5 rounded-badge bg-apple-orange/10 text-apple-orange">
                    {candidateStateLabel(candidate.state)}
                  </span>
                </div>
                <p className="mt-1 text-[11px] leading-relaxed text-secondary-light dark:text-secondary-dark break-words">
                  {candidatePreview(candidate)}
                </p>
                <p className="mt-1 text-[10px] font-medium text-apple-blue">
                  {candidateNextStep(candidate)}
                </p>
                <div className="mt-2 flex items-center justify-between gap-2 text-[10px] text-secondary-light dark:text-secondary-dark">
                  <span>Created {formatRelativeTime(candidate.createdAt)}</span>
                  {candidate.sourceRunId && <span>Suggested from this task</span>}
                </div>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}

function sectionDescription(kind: ContextCandidateKind): string {
  return kind === 'skill'
    ? 'These are instruction suggestions. Review them before agents can follow the workflow.'
    : 'These memory suggestions are not saved for future work until someone reviews them.'
}

function candidateTitle(candidate: TaskContextCandidate): string {
  const preview = candidate.proposedPreview
  for (const key of ['title', 'name', 'description']) {
    const value = preview[key]
    if (typeof value === 'string' && value.trim().length > 0) return value
  }
  switch (candidate.itemKind) {
    case 'memory':
      return 'Suggested memory'
    case 'skill':
      return 'Suggested saved instruction'
    default:
      return 'Suggested context item'
  }
}

function candidatePreview(candidate: TaskContextCandidate): string {
  const value = candidate.proposedPreview.content_preview
  return typeof value === 'string' && value.trim().length > 0
    ? value
    : 'No preview is available yet. Open saved item review to inspect the full suggestion.'
}

function candidateKindLabel(candidate: TaskContextCandidate): string {
  switch (candidate.itemKind) {
    case 'memory':
      return 'Suggested memory'
    case 'skill':
      return 'Instruction suggestion'
    default:
      return 'Suggestion needs review'
  }
}

function candidateStateLabel(state: TaskContextCandidate['state']): string {
  if (state === 'approved') return 'Approved'
  if (state === 'rejected') return 'Rejected'
  if (state === 'superseded') return 'Replaced'
  return 'Waiting for review'
}

function candidateNextStep(candidate: TaskContextCandidate): string {
  switch (candidate.itemKind) {
    case 'memory':
      return 'Next step: review the wording before saving it for future tasks.'
    case 'skill':
      return 'Next step: review the suggestion before agents can follow it.'
    default:
      return 'Next step: review this suggestion before agents can reuse it.'
  }
}
