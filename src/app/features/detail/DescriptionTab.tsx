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

interface DescriptionTabProps {
  task: TaskSummary
  onOpenContext?: () => void
  onOpenResult?: () => void
  onDraftSkill?: () => void
}

const HANDOFF_REVIEW_POINTS = [
  { label: 'Outcome', value: 'Confirm the result solves the original request.' },
  { label: 'Check work', value: 'Open result files or what the agent used before accepting.' },
  {
    label: 'Reuse',
    value: 'Save the repeatable steps only when they should help future tasks.',
  },
]

export function DescriptionTab({
  task,
  onOpenContext,
  onOpenResult,
  onDraftSkill,
}: DescriptionTabProps) {
  const resultArtifacts = taskResultArtifacts(task.result)
  const contextTotal = task.contextCounts?.total ?? 0
  const canReview = task.state === 'completed' || task.state === 'failed'
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

      <ReviewSection title="Next action" Icon={ListChecks}>
        <div className="space-y-1.5 text-xs">
          <div
            data-testid="task-next-action"
            className={cn(
              'rounded-lg px-2 py-1.5',
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
          <div data-testid="task-handoff-checklist" className="grid gap-1.5 text-xs sm:grid-cols-3">
            {HANDOFF_REVIEW_POINTS.map((point) => (
              <div
                key={point.label}
                className="min-w-0 rounded-lg bg-white px-2 py-1.5 dark:bg-black/20"
              >
                <span className="block text-[10px] font-medium text-secondary-light dark:text-secondary-dark">
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
        <div className="space-y-1.5 text-xs">
          <ReviewRow label="Agent" value={assignment.label} muted={!assignment.hasAgent} />
          <ReviewRow label="State" value={taskStateLabel(task.state)} />
          <p
            data-testid="task-assignment-guidance"
            className={cn(
              'rounded-lg px-2 py-1.5 leading-relaxed',
              assignment.hasAgent
                ? 'bg-apple-blue/10 text-foreground-light dark:text-foreground-dark'
                : 'bg-apple-orange/10 text-apple-orange'
            )}
          >
            {assignment.detail}
          </p>
          {blockedPreview && (
            <p
              data-testid="task-assignment-blocked-guidance"
              className="rounded-lg bg-apple-red/10 px-2 py-1.5 text-apple-red"
            >
              {blockedPreview}
            </p>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Task progress" Icon={Clock3}>
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
          {failurePreview && (
            <p className="rounded-lg bg-apple-red/10 px-2 py-1.5 text-apple-red">
              {failurePreview}
            </p>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Result files and evidence" Icon={CheckCircle2}>
        <div className="space-y-2 text-xs text-secondary-light dark:text-secondary-dark">
          <p>
            {resultArtifacts.length > 0
              ? `${resultArtifacts.length} result file${resultArtifacts.length === 1 ? '' : 's'} ready for review.`
              : canReview
                ? 'No result files were attached.'
                : 'Result files appear here after the run finishes.'}
          </p>
          {resultArtifacts.length > 0 && (
            <button
              type="button"
              onClick={onOpenResult}
              className="inline-flex h-8 items-center gap-1.5 rounded-full bg-apple-blue/10 px-3 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/15"
            >
              <span>Open result files</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
          <p>
            {contextTotal > 0
              ? `${contextTotal} saved ${
                  contextTotal === 1 ? 'note or instruction' : 'notes or instructions'
                } helped this task.`
              : 'Saved notes, run details, and save-for-next-time ideas appear here as the task runs.'}
          </p>
          {onOpenContext && (
            <button
              type="button"
              onClick={onOpenContext}
              className="inline-flex h-8 items-center gap-1.5 rounded-full bg-black/[0.04] px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.08] dark:bg-white/[0.06] dark:text-foreground-dark dark:hover:bg-white/[0.1]"
            >
              <span>Review what was used</span>
              <ArrowRight size={13} strokeWidth={2.25} aria-hidden="true" />
            </button>
          )}
        </div>
      </ReviewSection>

      <ReviewSection title="Reuse what worked" Icon={WandSparkles}>
        <div className="space-y-2 text-xs text-secondary-light dark:text-secondary-dark">
          <p>
            {task.state === 'completed'
              ? 'After review, save the repeatable steps if future tasks should reuse them.'
              : 'The save-for-next-time path becomes available once useful work is completed.'}
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
                  <span>Review save ideas</span>
                </button>
              )}
              {onDraftSkill && (
                <button
                  type="button"
                  onClick={onDraftSkill}
                  className="inline-flex h-8 items-center gap-1.5 rounded-full bg-black/[0.04] px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.08] dark:bg-white/[0.06] dark:text-foreground-dark dark:hover:bg-white/[0.1]"
                >
                  <span>Draft saved instruction</span>
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

function assignmentSummary(task: TaskSummary): {
  label: string
  detail: string
  hasAgent: boolean
} {
  if (task.assignedAgentName) {
    return {
      label: task.assignedAgentName,
      detail: 'This agent will handle the next run for this task.',
      hasAgent: true,
    }
  }
  if (task.assignedTo) {
    return {
      label: 'Agent details loading',
      detail: 'An agent was chosen, but its display name has not loaded yet.',
      hasAgent: true,
    }
  }
  return {
    label: 'Needs agent',
    detail: 'Choose an agent before this task can start.',
    hasAgent: false,
  }
}

function nextActionForTask(
  task: TaskSummary,
  artifactCount: number,
  contextTotal: number
): { title: string; detail: string; tone: 'default' | 'success' | 'warn' } {
  switch (task.state) {
    case 'backlog':
      return task.assignedTo || task.assignedAgentName
        ? {
            title: 'Ready to send',
            detail: 'Review the brief, then send it to this agent.',
            tone: 'default',
          }
        : {
            title: 'Assign an agent',
            detail:
              'Choose an available agent, review the suggested saved notes and instructions, then send the task.',
            tone: 'warn',
          }
    case 'queued':
      return {
        title: 'Waiting for the agent to start',
        detail: 'Keep the brief current while the chosen agent gets ready to start.',
        tone: 'default',
      }
    case 'working':
      return {
        title: 'Monitor progress',
        detail:
          task.progress >= 80
            ? 'Prepare to review result files when the agent completes the run.'
            : 'Watch progress and use Needs help if the agent needs your input.',
        tone: 'default',
      }
    case 'blocked':
      return {
        title: 'Provide what is missing',
        detail: taskBlockedPreview({
          blockedHint: task.blockedHint,
          blockedReason: task.blockedReason,
          error: task.error,
        }),
        tone: 'warn',
      }
    case 'completed':
      return {
        title: 'Review the handoff',
        detail:
          artifactCount > 0
            ? 'Open result files, check what the agent reused, and save repeatable steps if future tasks should use them.'
            : contextTotal > 0
              ? 'Check what the agent reused, then save repeatable steps if future tasks should use them.'
              : 'Confirm the outcome, then save repeatable steps or create a follow-up task if something is missing.',
        tone: 'success',
      }
    case 'failed':
      return {
        title: 'Triage failure',
        detail: taskFailurePreview(task.error),
        tone: 'warn',
      }
    case 'canceled':
      return {
        title: 'No active run',
        detail: 'Create a new task or reopen the brief if this work still matters.',
        tone: 'default',
      }
    default:
      return {
        title: 'Check current status',
        detail: 'Open Updates to review the latest activity before starting, retrying, or closing.',
        tone: 'warn',
      }
  }
}
