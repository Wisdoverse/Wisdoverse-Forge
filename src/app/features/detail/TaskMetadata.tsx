import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { taskMachineKey, taskPriorityLabel, taskStateLabel } from '@app/entities/task'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'

const STATE_COLORS: Record<string, string> = {
  backlog: 'bg-apple-gray-1 text-white',
  queued: 'bg-apple-orange text-white',
  working: 'bg-apple-green text-white',
  blocked: 'bg-apple-red text-white',
  completed: 'bg-apple-gray-2 text-white',
  failed: 'bg-apple-red text-white',
  canceled: 'bg-apple-gray-3 text-white',
}

const PRIORITY_COLORS: Record<string, string> = {
  urgent: 'bg-apple-red/10 text-apple-red',
  high: 'bg-apple-orange/10 text-apple-orange',
  normal: 'bg-apple-gray-5 text-apple-gray-1',
  low: 'bg-apple-gray-5 text-apple-gray-2',
}

interface TaskMetadataProps {
  task: TaskSummary
}

export function TaskMetadata({ task }: TaskMetadataProps) {
  const hasAssignee = Boolean(task.assignedAgentName || task.assignedTo)
  const guidance = taskMetadataGuidance(task, hasAssignee)
  const stateKey = taskMachineKey(task.state)
  const priorityKey = taskMachineKey(task.priority)

  return (
    <div className="flex flex-col gap-3 py-3">
      {/* Badges row */}
      <div className="flex items-center gap-2 flex-wrap">
        <span
          className={cn(
            'text-[10px] font-semibold px-2 py-0.5 rounded-badge',
            STATE_COLORS[stateKey] ?? 'bg-apple-gray-5 text-apple-gray-1'
          )}
        >
          {taskStateLabel(task.state)}
        </span>
        <span
          className={cn(
            'text-[10px] font-medium px-1.5 py-0.5 rounded-badge',
            PRIORITY_COLORS[priorityKey] ?? 'bg-apple-gray-5 text-apple-gray-1'
          )}
        >
          {taskPriorityLabel(task.priority)}
        </span>
      </div>

      {/* Agent */}
      <div className="flex items-center justify-between text-xs">
        <span className="text-secondary-light dark:text-secondary-dark">Agent</span>
        {hasAssignee ? (
          <span className="font-medium text-apple-purple">
            {task.assignedAgentName ?? 'Agent details loading'}
          </span>
        ) : (
          <span className="text-secondary-light dark:text-secondary-dark">Needs agent</span>
        )}
      </div>

      <div
        data-testid="task-metadata-guidance"
        className="space-y-1 rounded-lg border border-black/[0.06] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.04]"
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
              className="h-full bg-apple-green rounded-full transition-all"
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

function taskMetadataGuidance(task: TaskSummary, hasAssignee: boolean): string {
  switch (task.state) {
    case 'backlog':
      return hasAssignee
        ? 'This task is prepared but not started. Preview the saved notes, then send it to the agent.'
        : 'This task is still a draft. Choose an agent before it can start.'
    case 'queued':
      return hasAssignee
        ? 'The task is waiting for the chosen agent to start.'
        : 'The task is waiting for the next available agent to start.'
    case 'working':
      return 'An agent is working now. Watch progress here and check Updates for recent activity.'
    case 'blocked':
      return taskBlockedPreview({
        blockedHint: task.blockedHint,
        blockedReason: task.blockedReason,
        error: task.error,
      })
    case 'completed':
      return 'The task is finished. Review the Result tab or the final answer before closing the loop.'
    case 'failed':
      return taskFailurePreview(task.error)
    case 'canceled':
      return 'The task was stopped intentionally. Open Updates to see the last recorded activity.'
    default:
      return 'Open Updates to review the latest task activity.'
  }
}
