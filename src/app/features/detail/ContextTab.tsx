import { useEffect, useMemo, useState } from 'react'
import { CheckCircle2, Info } from 'lucide-react'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'
import { ContextAppliedList } from './ContextAppliedList'
import { ContextCandidatesList } from './ContextCandidatesList'
import { ContextEvidenceList } from './ContextEvidenceList'
import { taskDetailErrorMessage } from './taskDetailErrorMessages'
import type {
  AppliedContextItem,
  ContextFeedbackLabel,
  ContextFeedbackOutcome,
  MemoryContent,
  TaskContextResponse,
} from '@shared/types/context'

interface ContextTabProps {
  taskId: string
  loadContext?: (taskId: string) => Promise<TaskContextResponse>
  readMemoryContent?: (memoryId: string) => Promise<MemoryContent>
  recordFeedback?: (
    item: AppliedContextItem,
    label: ContextFeedbackLabel
  ) => Promise<ContextFeedbackOutcome>
}

const EMPTY_CONTEXT_STEPS = [
  'Publish or run the task so Forge can choose memories and skills.',
  'Open suggested memory updates after a run to keep useful context for next time.',
  'Use feedback on applied items so future runs learn what helped.',
]

export function ContextTab({
  taskId,
  loadContext = orchestrationApi.fetchContextForTask,
  readMemoryContent = orchestrationApi.readMemoryContent,
  recordFeedback: recordFeedbackProp,
}: ContextTabProps) {
  const [context, setContext] = useState<TaskContextResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let canceled = false
    setLoading(true)
    setError(null)
    loadContext(taskId)
      .then((nextContext) => {
        if (!canceled) setContext(nextContext)
      })
      .catch((err) => {
        if (!canceled) setError(taskDetailErrorMessage('loadContext', err))
      })
      .finally(() => {
        if (!canceled) setLoading(false)
      })
    return () => {
      canceled = true
    }
  }, [loadContext, taskId])

  const grouped = useMemo(() => {
    const applied = context?.appliedItems ?? []
    return {
      memories: applied.filter((item) => item.itemKind === 'memory'),
      skills: applied.filter((item) => item.itemKind === 'skill'),
      revoked: applied.filter((item) => item.revoked),
    }
  }, [context])

  function markFeedback(item: AppliedContextItem, label: ContextFeedbackLabel) {
    setContext((current) => {
      if (!current) return current
      return {
        ...current,
        appliedItems: current.appliedItems.map((candidate) =>
          candidate.injectionId === item.injectionId
            ? {
                ...candidate,
                feedback: {
                  label,
                  note: null,
                  updatedAt: new Date().toISOString(),
                },
              }
            : candidate
        ),
      }
    })
  }

  const recordFeedback =
    recordFeedbackProp ??
    (async (item: AppliedContextItem, label: ContextFeedbackLabel) => {
      const outcome = await orchestrationApi.recordContextFeedback({
        run_id: item.runId,
        item_id: item.itemId,
        item_kind: item.itemKind,
        label,
      })
      markFeedback(item, label)
      return outcome
    })

  if (loading) {
    return (
      <div className="py-8 flex items-center justify-center">
        <p className="text-xs text-secondary-light dark:text-secondary-dark">
          Loading saved context...
        </p>
      </div>
    )
  }

  if (error) {
    return (
      <div className="py-8 flex items-center justify-center">
        <p role="alert" className="text-xs text-apple-red">
          {error}
        </p>
      </div>
    )
  }

  if (!context || isEmptyContext(context)) {
    return <ContextEmptyState />
  }

  return (
    <div className="py-3 space-y-4" data-testid="context-tab">
      {context.runs.length > 0 && (
        <section className="rounded-lg bg-apple-gray-6/70 dark:bg-white/[0.035] p-3">
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
              Agent work checked for context
            </h3>
            <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
              {context.runs.length} record{context.runs.length === 1 ? '' : 's'}
            </span>
          </div>
          <div className="mt-2 space-y-1.5">
            {context.runs.map((run, index) => (
              <div
                key={run.id}
                className="flex items-center justify-between gap-2 text-[10px] text-secondary-light dark:text-secondary-dark"
              >
                <span className="font-medium text-foreground-light dark:text-foreground-dark">
                  Work run {index + 1}
                </span>
                <span>{runStatusLabel(run.status)}</span>
                <span>Started {formatRelativeTime(run.startedAt)}</span>
              </div>
            ))}
          </div>
        </section>
      )}

      <ContextAppliedList
        title="Applied memories"
        kind="memory"
        items={grouped.memories}
        onReadMemoryContent={readMemoryContent}
        onRecordFeedback={(item, label) => recordFeedback(item, label)}
      />
      <ContextCandidatesList
        title="Suggested memory updates"
        kind="memory"
        candidates={context.suggestedMemoryUpdates}
      />
      <ContextAppliedList
        title="Applied skills"
        kind="skill"
        items={grouped.skills}
        onReadMemoryContent={readMemoryContent}
        onRecordFeedback={(item, label) => recordFeedback(item, label)}
      />
      <ContextCandidatesList
        title="Suggested skills to review"
        kind="skill"
        candidates={context.skillCandidates}
      />
      <ContextEvidenceList evidence={context.evidence} revokedItems={grouped.revoked} />
      {context.provenance.length > 0 && (
        <section className="space-y-2" data-testid="context-provenance">
          <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
            Where saved context came from
          </h3>
          <div className="space-y-1.5">
            {context.provenance.map((item) => (
              <div
                key={`${item.runId}-${item.itemId}`}
                className="rounded-lg bg-apple-gray-6/70 dark:bg-white/[0.035] px-3 py-2 text-[10px] text-secondary-light dark:text-secondary-dark"
              >
                <span className="font-medium text-foreground-light dark:text-foreground-dark">
                  {item.title}
                </span>{' '}
                came from {contextSourceLabel(item.source)} and was used during this agent run.
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

function runStatusLabel(status: string): string {
  switch (status) {
    case 'completed':
      return 'Finished'
    case 'running':
    case 'working':
      return 'In progress'
    case 'failed':
      return 'Needs review'
    case 'canceled':
    case 'cancelled':
      return 'Stopped'
    default:
      return status
        .split(/[_\s-]+/)
        .filter(Boolean)
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(' ')
  }
}

function contextSourceLabel(source: TaskContextResponse['provenance'][number]['source']): string {
  return source?.title ?? 'the context selection step'
}

function ContextEmptyState() {
  return (
    <section
      className="py-6"
      data-testid="context-empty-state"
      aria-labelledby="context-empty-title"
    >
      <div className="mx-auto flex max-w-2xl flex-col gap-3">
        <div className="flex items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-apple-blue/10 text-apple-blue">
            <Info size={17} strokeWidth={2.15} aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h3
              id="context-empty-title"
              className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark"
            >
              No context has been applied yet
            </h3>
            <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
              Context appears here after an agent run uses saved memories, reusable skills, or
              evidence for this task.
            </p>
          </div>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {EMPTY_CONTEXT_STEPS.map((step) => (
            <div
              key={step}
              className="flex min-h-16 items-start gap-2 rounded-lg bg-apple-gray-6/70 px-3 py-2 dark:bg-white/[0.035]"
            >
              <CheckCircle2
                size={14}
                strokeWidth={2.15}
                className="mt-0.5 shrink-0 text-apple-green"
                aria-hidden="true"
              />
              <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                {step}
              </span>
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}

function isEmptyContext(context: TaskContextResponse): boolean {
  return (
    context.appliedItems.length === 0 &&
    context.suggestedMemoryUpdates.length === 0 &&
    context.skillCandidates.length === 0 &&
    context.evidence.length === 0 &&
    context.provenance.length === 0
  )
}
