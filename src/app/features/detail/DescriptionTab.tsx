import type { ComponentType, ReactNode, SVGProps } from 'react'
import {
  ArrowRight,
  CheckCircle2,
  Clock3,
  FileText,
  ListChecks,
  MessageSquare,
  WandSparkles,
} from 'lucide-react'
import { taskResultArtifacts, type TaskSummary } from '@app/shared/api/orchestration'
import { taskStateLabel } from '@app/entities/task'
import { formatRelativeTime } from '@app/shared/lib/time'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import {
  HANDOFF_REVIEW_POINTS,
  assignmentSummary,
  missingBriefCopy,
  nextActionForTask,
  taskHasBrief,
} from './model/taskGuidance'

interface DescriptionTabProps {
  task: TaskSummary
  onOpenContext?: () => void
  onOpenResult?: () => void
  onDraftSkill?: () => void
  showAssignmentAction?: boolean
}

export function DescriptionTab({
  task,
  onOpenContext,
  onOpenResult,
  onDraftSkill,
  showAssignmentAction = true,
}: DescriptionTabProps) {
  const resultArtifacts = taskResultArtifacts(task.result)
  const contextTotal = task.contextCounts?.total ?? 0
  const canReview = task.state === 'completed' || task.state === 'failed'
  const hasBrief = taskHasBrief(task)
  const nextAction = nextActionForTask(task, resultArtifacts.length, contextTotal)
  const assignment = assignmentSummary(task)
  const failurePreview = task.error ? taskFailurePreview(task.error) : null
  const blockedPreview =
    task.state === 'blocked' && task.blockedHint
      ? taskBlockedPreview({
          blockedHint: task.blockedHint,
          blockedReason: task.blockedReason,
          error: task.error,
        })
      : null

  return (
    <div className="space-y-3 py-3" data-testid="task-work-review">
      <ReviewSection title="Brief" Icon={FileText}>
        {hasBrief ? (
          <p className="whitespace-pre-wrap text-ui-body leading-relaxed text-foreground-light dark:text-foreground-dark">
            {task.params.message}
          </p>
        ) : (
          <p className="text-ui-body italic text-secondary-light dark:text-secondary-dark">
            {missingBriefCopy(task)}
          </p>
        )}
      </ReviewSection>

      <ReviewSection title="Next action" Icon={ListChecks}>
        <div className="space-y-1.5 text-ui-body">
          <div
            data-testid="task-next-action"
            className={cn(
              'rounded-card px-2 py-1.5',
              nextAction.tone === 'warn'
                ? 'bg-apple-orange/10 text-apple-orange'
                : nextAction.tone === 'success'
                  ? 'bg-apple-green/10 text-apple-green'
                  : 'bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
            )}
          >
            <p className="font-semibold">{nextAction.title}</p>
            <p className="mt-0.5 leading-relaxed">{nextAction.detail}</p>
          </div>
        </div>
      </ReviewSection>

      {task.state === 'completed' && (
        <ReviewSection title="Handoff checklist" Icon={CheckCircle2}>
          <div
            data-testid="task-handoff-checklist"
            className="grid gap-1.5 text-ui-body sm:grid-cols-3"
          >
            {HANDOFF_REVIEW_POINTS.map((point) => (
              <div
                key={point.label}
                className="min-w-0 rounded-card bg-white px-2 py-1.5 dark:bg-black/20"
              >
                <span className="block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
                  {point.label}
                </span>
                <span className="mt-0.5 block leading-relaxed text-foreground-light dark:text-foreground-dark">
                  {point.value}
                </span>
              </div>
            ))}
          </div>
        </ReviewSection>
      )}

      <ReviewSection title="Assignment" Icon={MessageSquare}>
        <div className="space-y-1.5 text-ui-body">
          <ReviewRow label="Agent" value={assignment.label} muted={!assignment.hasAgent} />
          <ReviewRow label="State" value={taskStateLabel(task.state)} />
          <p
            data-testid="task-assignment-guidance"
            className={cn(
              'rounded-card px-2 py-1.5 leading-relaxed',
              assignment.hasAgent
                ? 'bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                : 'bg-apple-orange/10 text-apple-orange'
            )}
          >
            {assignment.detail}
          </p>
          {!assignment.hasAgent && showAssignmentAction && (
            <a href="/agents" className={cn(uiStyles.secondaryButton, 'w-fit text-apple-blue')}>
              <span>Open Agents</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </a>
          )}
          {blockedPreview && (
            <p
              data-testid="task-assignment-blocked-guidance"
              className="rounded-card bg-apple-red/10 px-2 py-1.5 text-apple-red"
            >
              {blockedPreview}
            </p>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Task progress" Icon={Clock3}>
        <div className="space-y-1.5 text-ui-body">
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
          {failurePreview && (
            <p className="rounded-card bg-apple-red/10 px-2 py-1.5 text-apple-red">
              {failurePreview}
            </p>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Result files" Icon={CheckCircle2}>
        <div className="space-y-2 text-ui-body text-secondary-light dark:text-secondary-dark">
          <p>
            {resultArtifacts.length > 0
              ? `${resultArtifacts.length} result file${resultArtifacts.length === 1 ? '' : 's'} ready to check.`
              : canReview
                ? 'No result files were saved. Use Next action above, then retry or create a follow-up task if files are still needed.'
                : 'Result files appear here after the task finishes.'}
          </p>
          {resultArtifacts.length > 0 && (
            <button
              type="button"
              onClick={onOpenResult}
              className={cn(uiStyles.secondaryButton, 'text-apple-blue')}
            >
              <span>Open result files</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
          <p>
            {contextTotal > 0
              ? `${contextTotal} context item${contextTotal === 1 ? '' : 's'} helped this task.`
              : 'Saved notes, work history, and ideas to reuse next time appear here while the task is active.'}
          </p>
          {onOpenContext && (
            <button type="button" onClick={onOpenContext} className={uiStyles.secondaryButton}>
              <span>Check what was used</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Reuse what worked" Icon={WandSparkles}>
        <div className="space-y-2 text-ui-body text-secondary-light dark:text-secondary-dark">
          <p>
            {task.state === 'completed'
              ? 'After checking the result, save the repeatable steps if future tasks should reuse them.'
              : 'You can save repeatable steps after useful work is completed.'}
          </p>
          {task.state === 'completed' && (
            <div className="flex flex-wrap gap-2">
              {onOpenContext && (
                <button type="button" onClick={onOpenContext} className={uiStyles.primaryButton}>
                  <WandSparkles size={13} strokeWidth={2.25} aria-hidden="true" />
                  <span>Check ideas to reuse</span>
                </button>
              )}
              {onDraftSkill && (
                <button type="button" onClick={onDraftSkill} className={uiStyles.secondaryButton}>
                  <span>Draft a skill</span>
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
    <section className="rounded-card bg-apple-gray-6/70 p-3 dark:bg-white/[0.035]">
      <div className="mb-2 flex items-center gap-2">
        <Icon size={14} strokeWidth={2.25} className="text-apple-blue" aria-hidden="true" />
        <h3 className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          {title}
        </h3>
      </div>
      {children}
    </section>
  )
}

function ReviewRow({
  label,
  value,
  muted = false,
}: {
  label: string
  value: string
  muted?: boolean
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-secondary-light dark:text-secondary-dark">{label}</span>
      <span
        className={cn(
          'min-w-0 truncate text-right font-medium',
          muted
            ? 'text-secondary-light dark:text-secondary-dark'
            : 'text-foreground-light dark:text-foreground-dark'
        )}
      >
        {value}
      </span>
    </div>
  )
}
