import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { TaskSummary } from '@app/shared/api/orchestration'

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

const PRIORITY_LABELS: Record<string, string> = {
  urgent: 'Urgent',
  high: 'High',
  normal: 'Normal',
  low: 'Low',
}

const STATE_LABELS: Record<string, string> = {
  backlog: 'Backlog',
  queued: 'Queued',
  working: 'Working',
  blocked: 'Blocked',
  completed: 'Completed',
  failed: 'Failed',
  canceled: 'Canceled',
}

interface TaskMetadataProps {
  task: TaskSummary
}

export function TaskMetadata({ task }: TaskMetadataProps) {
  return (
    <div className="flex flex-col gap-3 py-3">
      {/* Badges row */}
      <div className="flex items-center gap-2 flex-wrap">
        <span
          className={cn(
            'text-[10px] font-semibold px-2 py-0.5 rounded-badge',
            STATE_COLORS[task.state] ?? 'bg-apple-gray-5 text-apple-gray-1'
          )}
        >
          {STATE_LABELS[task.state] ?? task.state}
        </span>
        <span
          className={cn(
            'text-[10px] font-medium px-1.5 py-0.5 rounded-badge',
            PRIORITY_COLORS[task.priority] ?? 'bg-apple-gray-5 text-apple-gray-1'
          )}
        >
          {PRIORITY_LABELS[task.priority] ?? task.priority}
        </span>
      </div>

      {/* Assignee */}
      <div className="flex items-center justify-between text-xs">
        <span className="text-secondary-light dark:text-secondary-dark">Assigned to</span>
        {task.assignedAgentName ? (
          <span className="font-medium text-apple-purple">{task.assignedAgentName}</span>
        ) : (
          <span className="text-secondary-light dark:text-secondary-dark">Unassigned</span>
        )}
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
