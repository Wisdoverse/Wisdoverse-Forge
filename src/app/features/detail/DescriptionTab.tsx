import type { ComponentType, ReactNode, SVGProps } from 'react'
import {
  ArrowRight,
  CheckCircle2,
  Clock3,
  FileText,
  MessageSquare,
  WandSparkles,
} from 'lucide-react'
import { taskResultArtifacts, type TaskSummary } from '@app/shared/api/orchestration'
import { formatRelativeTime } from '@app/shared/lib/time'
import { cn } from '@app/shared/lib/utils'

interface DescriptionTabProps {
  task: TaskSummary
  onOpenContext?: () => void
  onOpenResult?: () => void
  onDraftSkill?: () => void
}

export function DescriptionTab({
  task,
  onOpenContext,
  onOpenResult,
  onDraftSkill,
}: DescriptionTabProps) {
  const resultArtifacts = taskResultArtifacts(task.result)
  const contextTotal = task.contextCounts?.total ?? 0
  const canReview = task.state === 'completed' || task.state === 'failed'

  return (
    <div className="space-y-3 py-3" data-testid="task-work-review">
      <ReviewSection title="Brief" Icon={FileText}>
        {task.params.message ? (
          <p className="whitespace-pre-wrap text-xs leading-relaxed text-foreground-light dark:text-foreground-dark">
            {task.params.message}
          </p>
        ) : (
          <p className="text-xs italic text-secondary-light dark:text-secondary-dark">
            No description provided.
          </p>
        )}
      </ReviewSection>

      <ReviewSection title="Assignment" Icon={MessageSquare}>
        <div className="space-y-1.5 text-xs">
          <ReviewRow label="Agent" value={task.assignedAgentName ?? 'Unassigned'} />
          <ReviewRow label="State" value={stateLabel(task.state)} />
          {task.blockedHint && (
            <p className="rounded-lg bg-apple-red/10 px-2 py-1.5 text-apple-red">
              {task.blockedHint}
            </p>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Execution log" Icon={Clock3}>
        <div className="space-y-1.5 text-xs">
          <ReviewRow label="Created" value={formatRelativeTime(task.createdAt)} />
          <ReviewRow label="Updated" value={formatRelativeTime(task.updatedAt)} />
          {task.completedAt && (
            <ReviewRow label="Completed" value={formatRelativeTime(task.completedAt)} />
          )}
          {task.state === 'working' && (
            <div>
              <div className="mb-1 flex items-center justify-between">
                <span className="text-secondary-light dark:text-secondary-dark">Progress</span>
                <span className="font-medium text-foreground-light dark:text-foreground-dark">
                  {task.progress}%
                </span>
              </div>
              <div className="h-1.5 overflow-hidden rounded-full bg-apple-gray-5 dark:bg-white/10">
                <div
                  className="h-full rounded-full bg-apple-blue transition-all"
                  style={{ width: `${task.progress}%` }}
                />
              </div>
            </div>
          )}
          {task.error && (
            <p className="rounded-lg bg-apple-red/10 px-2 py-1.5 text-apple-red">{task.error}</p>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Artifacts and evidence" Icon={CheckCircle2}>
        <div className="space-y-2 text-xs text-secondary-light dark:text-secondary-dark">
          <p>
            {resultArtifacts.length > 0
              ? `${resultArtifacts.length} result artifact${resultArtifacts.length === 1 ? '' : 's'} ready for review.`
              : canReview
                ? 'No result artifacts were attached.'
                : 'Result artifacts appear here after the run finishes.'}
          </p>
          {resultArtifacts.length > 0 && (
            <button
              type="button"
              onClick={onOpenResult}
              className="inline-flex h-8 items-center gap-1.5 rounded-full bg-apple-blue/10 px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/15"
            >
              <span>Open artifacts</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
          <p>
            {contextTotal > 0
              ? `${contextTotal} context item${contextTotal === 1 ? '' : 's'} applied to this task.`
              : 'Context, evidence, and skill candidates are collected as the run produces them.'}
          </p>
          {onOpenContext && (
            <button
              type="button"
              onClick={onOpenContext}
              className="inline-flex h-8 items-center gap-1.5 rounded-full bg-black/[0.04] px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.08] dark:bg-white/[0.06] dark:text-foreground-dark dark:hover:bg-white/[0.1]"
            >
              <span>Review context</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Reusable learning" Icon={WandSparkles}>
        <div className="space-y-2 text-xs text-secondary-light dark:text-secondary-dark">
          <p>
            {task.state === 'completed'
              ? 'Completed work can become a governed skill after review.'
              : 'The save-as-skill path becomes available once useful work is completed.'}
          </p>
          {task.state === 'completed' && (
            <div className="flex flex-wrap gap-2">
              {onOpenContext && (
                <button
                  type="button"
                  onClick={onOpenContext}
                  className="inline-flex h-8 items-center gap-1.5 rounded-full bg-apple-blue px-3 text-ui-button font-medium text-white transition-colors hover:bg-apple-blue-focus"
                >
                  <WandSparkles size={13} strokeWidth={2.25} aria-hidden="true" />
                  <span>Review skill candidates</span>
                </button>
              )}
              {onDraftSkill && (
                <button
                  type="button"
                  onClick={onDraftSkill}
                  className="inline-flex h-8 items-center gap-1.5 rounded-full bg-black/[0.04] px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.08] dark:bg-white/[0.06] dark:text-foreground-dark dark:hover:bg-white/[0.1]"
                >
                  <span>Draft reusable skill</span>
                  <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
                </button>
              )}
            </div>
          )}
        </div>
      </ReviewSection>
    </div>
  )
}

function ReviewSection({
  title,
  Icon,
  children,
}: {
  title: string
  Icon: ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>
  children: ReactNode
}) {
  return (
    <section className="rounded-lg bg-apple-gray-6/70 p-3 dark:bg-white/[0.035]">
      <div className="mb-2 flex items-center gap-2">
        <Icon size={14} strokeWidth={2.25} className="text-apple-blue" aria-hidden="true" />
        <h3 className="text-xs font-semibold text-foreground-light dark:text-foreground-dark">
          {title}
        </h3>
      </div>
      {children}
    </section>
  )
}

function ReviewRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-secondary-light dark:text-secondary-dark">{label}</span>
      <span
        className={cn(
          'min-w-0 truncate text-right font-medium',
          value === 'Unassigned'
            ? 'text-secondary-light dark:text-secondary-dark'
            : 'text-foreground-light dark:text-foreground-dark'
        )}
      >
        {value}
      </span>
    </div>
  )
}

function stateLabel(state: TaskSummary['state']): string {
  switch (state) {
    case 'backlog':
      return 'Backlog'
    case 'queued':
      return 'Queued'
    case 'working':
      return 'Working'
    case 'blocked':
      return 'Blocked'
    case 'completed':
      return 'Completed'
    case 'failed':
      return 'Failed'
    case 'canceled':
      return 'Canceled'
  }
}
