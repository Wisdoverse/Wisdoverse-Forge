import { MarkdownContent } from '@app/shared/ui/markdown'
import { taskResultArtifacts, type TaskSummary } from '@app/shared/api/orchestration'
import { HANDOFF_REVIEW_POINTS, missingBriefCopy, nextActionForTask } from '../model/taskGuidance'

const SECTION_LABEL =
  'mb-2 mt-8 text-ui-caption font-medium uppercase tracking-wide text-secondary-light dark:text-secondary-dark'

export function TaskDocumentBody({ task }: { task: TaskSummary }) {
  const artifacts = taskResultArtifacts(task.result)
  const next = nextActionForTask(task, artifacts.length, task.contextCounts?.total ?? 0)
  const brief = task.params.message.trim()

  return (
    <div>
      <div
        data-testid="task-next-action"
        className="mt-4 flex items-start gap-2 rounded-card border border-black/[0.06] bg-black/[0.02] px-3 py-2 text-ui-body dark:border-white/[0.08] dark:bg-white/[0.04]"
      >
        <span className="font-medium text-foreground-light dark:text-foreground-dark">
          {next.title}
        </span>
        <span className="text-secondary-light dark:text-secondary-dark">{next.detail}</span>
      </div>

      <h2 className={SECTION_LABEL}>Brief</h2>
      {brief ? (
        <MarkdownContent text={brief} />
      ) : (
        <p
          data-testid="task-brief-empty"
          className="text-ui-body text-secondary-light dark:text-secondary-dark"
        >
          {missingBriefCopy(task)}
        </p>
      )}

      {artifacts.length > 0 && (
        <>
          <h2 className={SECTION_LABEL}>Result</h2>
          <div className="space-y-3">
            {artifacts.map((artifact, i) => (
              <div
                key={i}
                className="rounded-card border border-black/[0.06] bg-white p-3 dark:border-white/[0.08] dark:bg-surface-dark"
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                    {artifact.name}
                  </span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {resultFileKindLabel(artifact.mimeType)}
                  </span>
                </div>
                <p className="mb-2 text-ui-caption leading-relaxed text-secondary-light dark:text-secondary-dark">
                  Use this result to decide whether the task is done. If it does not answer the
                  brief, go back to Work and decide whether to retry, check saved notes and
                  guidance, or create a follow-up task.
                </p>
                {artifact.mimeType.trim().toLowerCase() === 'text/markdown' ? (
                  <MarkdownContent text={artifact.data} />
                ) : (
                  <pre className="max-h-[300px] overflow-y-auto whitespace-pre-wrap break-words font-mono text-ui-body leading-relaxed text-foreground-light dark:text-foreground-dark">
                    {artifact.data}
                  </pre>
                )}
              </div>
            ))}
          </div>
        </>
      )}

      {task.state === 'completed' && (
        <section data-testid="task-handoff-checklist">
          <h2 className={SECTION_LABEL}>Handoff checklist</h2>
          <ul className="list-disc space-y-1 pl-5 text-ui-body text-secondary-light dark:text-secondary-dark">
            {HANDOFF_REVIEW_POINTS.map((point) => (
              <li key={point.label}>
                <span className="font-medium text-foreground-light dark:text-foreground-dark">
                  {point.label}:
                </span>{' '}
                {point.value}
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}

function resultFileKindLabel(mimeType: string): string {
  const normalized = mimeType.trim().toLowerCase()
  if (!normalized) return 'Result file'
  if (normalized.startsWith('text/') || normalized.includes('markdown')) return 'Text result'
  if (normalized.includes('json')) return 'Data result'
  if (normalized.startsWith('image/')) return 'Image result'
  if (normalized === 'application/pdf') return 'PDF result'
  return 'Result file'
}
