import { ArrowRight, Lightbulb } from 'lucide-react'
import { formatRelativeTime } from '@app/shared/lib/time'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
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
        <h3 className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </h3>
        <p className="mt-0.5 text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
          {sectionDescription(kind)}
        </p>
      </div>
      <div className="space-y-2">
        {candidates.map((candidate) => {
          const title = candidateTitle(candidate)
          return (
            <article
              key={candidate.id}
              className="rounded-card border border-black/[0.05] bg-apple-gray-6/70 p-3 dark:border-white/[0.06] dark:bg-white/[0.035]"
            >
              <div className="flex items-start gap-2">
                <div className="mt-0.5 w-6 h-6 rounded-md bg-white dark:bg-white/[0.06] flex items-center justify-center text-apple-orange shrink-0">
                  <Lightbulb size={14} strokeWidth={2} aria-hidden="true" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 flex-wrap">
                    <h4 className="text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
                      {title}
                    </h4>
                    <span className={uiStyles.badge}>{candidateKindLabel(candidate)}</span>
                    <span className="inline-flex items-center gap-1.5 text-ui-body text-secondary-light dark:text-secondary-dark">
                      <span
                        aria-hidden="true"
                        className={cn(
                          'h-1.5 w-1.5 rounded-full',
                          candidateStateDot(candidate.state)
                        )}
                      />
                      {candidateStateLabel(candidate.state)}
                    </span>
                  </div>
                  <p className="mt-1 break-words text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
                    {candidatePreview(candidate)}
                  </p>
                  <p className="mt-1 text-ui-caption font-medium text-apple-blue">
                    {candidateNextStep(candidate)}
                  </p>
                  <a
                    href="/context"
                    aria-label={`Open Context for ${title}`}
                    className="mt-2 inline-flex items-center gap-1 text-ui-caption font-semibold text-apple-blue underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/30"
                  >
                    <span>Open Context</span>
                    <ArrowRight size={12} strokeWidth={2.25} aria-hidden="true" />
                  </a>
                  <div className="mt-2 flex items-center justify-between gap-2 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    <span>Created {formatRelativeTime(candidate.createdAt)}</span>
                    {candidate.sourceRunId && <span>From this task</span>}
                  </div>
                </div>
              </div>
            </article>
          )
        })}
      </div>
    </section>
  )
}

function sectionDescription(kind: ContextCandidateKind): string {
  return kind === 'skill'
    ? 'This is suggested guidance from this task. Check it before agents can follow it.'
    : 'These are suggested notes from this task. Check one before saving it for future tasks.'
}

function candidateTitle(candidate: TaskContextCandidate): string {
  const preview = candidate.proposedPreview
  for (const key of ['title', 'name', 'description']) {
    const value = preview[key]
    if (typeof value === 'string' && value.trim().length > 0) return value
  }
  switch (candidate.itemKind) {
    case 'memory':
      return 'Untitled suggested note'
    case 'skill':
      return 'Untitled suggested guidance'
    default:
      return 'Check suggested item'
  }
}

function candidatePreview(candidate: TaskContextCandidate): string {
  const value = candidate.proposedPreview.content_preview
  return typeof value === 'string' && value.trim().length > 0
    ? value
    : 'The summary is not ready yet. Open Context and read the full suggestion before using it.'
}

function candidateKindLabel(candidate: TaskContextCandidate): string {
  switch (candidate.itemKind) {
    case 'memory':
      return 'Suggested note'
    case 'skill':
      return 'Suggested guidance'
    default:
      return 'Check suggested item'
  }
}

function candidateStateLabel(state: TaskContextCandidate['state']): string {
  if (state === 'approved') return 'Saved'
  if (state === 'rejected') return 'Not saved'
  if (state === 'superseded') return 'Replaced'
  return 'Needs your check'
}

function candidateStateDot(state: TaskContextCandidate['state']): string {
  if (state === 'approved') return 'bg-apple-green'
  if (state === 'rejected') return 'bg-apple-red'
  if (state === 'superseded') return 'bg-apple-gray-3'
  return 'bg-apple-orange'
}

function candidateNextStep(candidate: TaskContextCandidate): string {
  switch (candidate.itemKind) {
    case 'memory':
      return 'Next step: open Context, then check the wording before saving it for future tasks.'
    case 'skill':
      return 'Next step: open Context, then check this guidance before agents can follow it.'
    default:
      return 'Next step: open Context, then check this suggestion before agents can reuse it.'
  }
}
