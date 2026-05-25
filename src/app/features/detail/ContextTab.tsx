import { useEffect, useMemo, useState } from 'react'
import { CheckCircle2, Clock3, ListChecks } from 'lucide-react'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'
import { ContextAppliedList } from './ContextAppliedList'
import { ContextCandidatesList } from './ContextCandidatesList'
import { ContextEvidenceList } from './ContextEvidenceList'
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
        if (!canceled) setError(err instanceof Error ? err.message : 'Could not load context')
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
        <p className="text-xs text-secondary-light dark:text-secondary-dark">Loading context...</p>
      </div>
    )
  }

  if (error) {
    return (
      <div className="py-8 flex items-center justify-center">
        <p className="text-xs text-apple-red">{error}</p>
      </div>
    )
  }

  if (!context || isEmptyContext(context)) {
    return (
      <div className="py-5" data-testid="context-empty-state">
        <div className="rounded-lg border border-black/[0.08] bg-apple-gray-6/70 p-4 dark:border-white/[0.08] dark:bg-white/[0.035]">
          <div className="flex items-start gap-3">
            <Clock3
              size={18}
              strokeWidth={2.15}
              className="mt-0.5 shrink-0 text-apple-blue"
              aria-hidden="true"
            />
            <div>
              <p className="text-sm font-semibold text-foreground-light dark:text-foreground-dark">
                Context will appear after useful run activity
              </p>
              <p className="mt-1 text-xs leading-relaxed text-secondary-light dark:text-secondary-dark">
                The agent has not applied memories, skills, evidence, or review candidates for this
                task yet.
              </p>
            </div>
          </div>
          <div className="mt-3 grid gap-2 text-xs text-secondary-light dark:text-secondary-dark sm:grid-cols-2">
            <div className="flex items-start gap-2 rounded-lg bg-white px-2.5 py-2 dark:bg-white/[0.04]">
              <ListChecks
                size={14}
                strokeWidth={2.15}
                className="mt-0.5 shrink-0 text-apple-blue"
                aria-hidden="true"
              />
              <span>Run or update the task brief if the agent needs more instructions.</span>
            </div>
            <div className="flex items-start gap-2 rounded-lg bg-white px-2.5 py-2 dark:bg-white/[0.04]">
              <CheckCircle2
                size={14}
                strokeWidth={2.15}
                className="mt-0.5 shrink-0 text-apple-green"
                aria-hidden="true"
              />
              <span>Return here after completion to review reusable context and evidence.</span>
            </div>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="py-3 space-y-4" data-testid="context-tab">
      {context.runs.length > 0 && (
        <section className="rounded-lg bg-apple-gray-6/70 dark:bg-white/[0.035] p-3">
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
              Runs
            </h3>
            <span className="text-[10px] text-secondary-light dark:text-secondary-dark">
              {context.runs.length} run{context.runs.length === 1 ? '' : 's'}
            </span>
          </div>
          <div className="mt-2 space-y-1.5">
            {context.runs.map((run) => (
              <div
                key={run.id}
                className="flex items-center justify-between gap-2 text-[10px] text-secondary-light dark:text-secondary-dark"
              >
                <span className="font-mono">{run.id.slice(0, 8)}</span>
                <span>{run.status}</span>
                <span>{formatRelativeTime(run.startedAt)}</span>
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
        title="Skill candidates"
        kind="skill"
        candidates={context.skillCandidates}
      />
      <ContextEvidenceList evidence={context.evidence} revokedItems={grouped.revoked} />
      {context.provenance.length > 0 && (
        <section className="space-y-2" data-testid="context-provenance">
          <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
            Provenance
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
                via {item.adapter} {item.envelopeVersion} from{' '}
                {item.source?.title ?? item.source?.sourceType ?? 'context resolver'}.
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
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
