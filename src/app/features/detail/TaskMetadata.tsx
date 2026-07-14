import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { taskPriorityLabel, taskStateLabel } from '@app/entities/task'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { TASK_AGENT_NAME_LOADING_LABEL } from './model/taskAgentLabels'

const METADATA_BADGE_TONE =
  'border border-black/[0.08] bg-transparent text-secondary-light dark:border-white/[0.1] dark:text-secondary-dark'

interface TaskMetadataProps {
  task: TaskSummary
}

export function TaskMetadata({ task }: TaskMetadataProps) {
  const hasAssignee = Boolean(task.assignedAgentName || task.assignedTo)
  const guidance = taskMetadataGuidance(task, hasAssignee)
  const attemptLabel = taskAttemptLabel(task.attempt)

  return (
    <div className="flex flex-col gap-3 py-3">
      {/* Badges row */}
      <div className="flex items-center gap-2 flex-wrap">
        <span
          className={cn('text-[10px] font-semibold px-2 py-0.5 rounded-badge', METADATA_BADGE_TONE)}
        >
          {taskStateLabel(task.state)}
        </span>
        <span
          className={cn('text-[10px] font-medium px-1.5 py-0.5 rounded-badge', METADATA_BADGE_TONE)}
        >
          {taskPriorityLabel(task.priority)}
        </span>
        {attemptLabel && (
          <span
            className={cn(
              'text-[10px] font-medium px-1.5 py-0.5 rounded-badge tabular-nums',
              METADATA_BADGE_TONE
            )}
          >
            {attemptLabel}
          </span>
        )}
      </div>

      {/* Agent check-in countdown while work is active. */}
      {task.state === 'working' && task.leaseExpiresAt != null && (
        <p className="text-[10px] text-secondary-light dark:text-secondary-dark">
          Agent should report back {formatRelativeTime(task.leaseExpiresAt)}
        </p>
      )}

      {/* Agent */}
      <div className="flex items-center justify-between text-xs">
        <span className="text-secondary-light dark:text-secondary-dark">Agent</span>
        {hasAssignee ? (
          <span className="font-medium text-foreground-light dark:text-foreground-dark">
            {task.assignedAgentName ?? TASK_AGENT_NAME_LOADING_LABEL}
          </span>
        ) : (
          <span className="text-secondary-light dark:text-secondary-dark">Needs agent</span>
        )}
      </div>

      <div
        data-testid="task-metadata-guidance"
        className="space-y-1 border-y border-black/[0.06] bg-transparent py-2 dark:border-white/[0.08]"
      >
        <p className="text-[10px] font-semibold uppercase tracking-wide text-secondary-light dark:text-secondary-dark">
          What this status means
        </p>
        <p className="text-xs leading-relaxed text-foreground-light dark:text-foreground-dark">
          {guidance}
        </p>
      </div>

      {/* Progress */}
      {task.state === 'working' && (
        <div>
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-secondary-light dark:text-secondary-dark">Progress</span>
            <span className="font-medium text-foreground-light dark:text-foreground-dark">
              {task.progress}%
            </span>
          </div>
          <div className="h-1.5 bg-apple-gray-5 dark:bg-white/10 rounded-full overflow-hidden">
            <div
              className="h-full rounded-full bg-secondary-light transition-all dark:bg-secondary-dark"
              style={{ width: `${task.progress}%` }}
            />
          </div>
        </div>
      )}

      {/* Timestamps */}
      <div className="flex items-center justify-between text-[10px] text-secondary-light dark:text-secondary-dark">
        <span>Created {formatRelativeTime(task.createdAt)}</span>
        <span>Updated {formatRelativeTime(task.updatedAt)}</span>
      </div>
    </div>
  )
}

function taskAttemptLabel(attempt: number): string | null {
  if (!Number.isInteger(attempt) || attempt < 1) return null
  if (attempt === 1) return 'First work try'
  return `Work try ${attempt}`
}

function taskMetadataGuidance(task: TaskSummary, hasAssignee: boolean): string {
  switch (task.state) {
    case 'backlog':
      return hasAssignee
        ? 'This task is prepared but not started. Preview the saved notes, then send it to the agent.'
        : 'This task is still a draft. Choose an agent before it can start.'
    case 'queued':
      return hasAssignee
        ? 'The task is waiting for the chosen agent to start. If it stays here, open Updates or choose another agent.'
        : 'The task is waiting for an agent to start. If it stays here, choose or start an agent.'
    case 'working':
      return 'An agent is working now. Watch progress here and check Updates for recent activity.'
    case 'blocked':
      return taskBlockedPreview({
        blockedHint: task.blockedHint,
        blockedReason: task.blockedReason,
        error: task.error,
      })
    case 'completed':
      return 'The task is finished. Check the Result tab or the final answer before closing the loop.'
    case 'failed':
      return taskFailurePreview(task.error)
    case 'canceled':
      return 'The task was stopped intentionally. Open Updates to see the latest saved activity.'
    default:
      return 'Open Updates to check the latest task activity.'
  }
}
