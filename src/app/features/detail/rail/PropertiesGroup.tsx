import { InjectionPreviewModal } from '@app/entities/context/ui/InjectionPreviewModal'
import { taskPriorityLabel, taskStateLabel } from '@app/entities/task'
import type { TaskSummary } from '@app/shared/api/orchestration'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useTaskActions } from '../model/useTaskActions'
import { RailRow, RailSection } from './RailSection'

const STATUS_DOTS: Record<string, string> = {
  backlog: 'bg-apple-gray-2',
  queued: 'bg-apple-gray-1',
  working: 'bg-foreground-light dark:bg-foreground-dark',
  blocked: 'bg-apple-red',
  completed: 'bg-apple-gray-2',
  failed: 'bg-apple-red',
  canceled: 'bg-apple-gray-3',
}

export function TaskStatus({ state }: { state: TaskSummary['state'] }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span aria-hidden="true" className={`h-1.5 w-1.5 rounded-full ${STATUS_DOTS[state]}`} />
      <span>{taskStateLabel(state)}</span>
    </span>
  )
}

export function PropertiesGroup({ task }: { task: TaskSummary }) {
  const actions = useTaskActions(task)

  return (
    <>
      <RailSection title="Properties">
        <RailRow label="Status">
          <TaskStatus state={task.state} />
        </RailRow>
        {(task.attempt ?? 1) > 1 && (
          <RailRow label="Attempt">
            {task.attempt} (retried {task.attempt - 1} time{task.attempt - 1 === 1 ? '' : 's'})
          </RailRow>
        )}
        {task.assignedAgentName && <RailRow label="Agent">{task.assignedAgentName}</RailRow>}
        {task.groupId && <RailRow label="Queue">{task.groupId}</RailRow>}
        <RailRow label="Priority">{taskPriorityLabel(task.priority)}</RailRow>

        {actions.canAssign && (
          <div className="grid gap-2 pt-1">
            <select
              aria-label="Available agent"
              value={actions.selectedAgentId}
              onChange={(event) => actions.setSelectedAgentId(event.target.value)}
              className={`${uiStyles.select} w-full`}
            >
              <option value="">Choose an agent</option>
              {actions.participants.map((participant) => (
                <option key={participant.agentId} value={participant.agentId}>
                  {participant.name}
                </option>
              ))}
            </select>
            <button
              type="button"
              disabled={!actions.selectedAgentId}
              onClick={() => void actions.assign(actions.selectedAgentId)}
              className={`${uiStyles.subtleButton} w-full`}
            >
              Preview and send
            </button>
          </div>
        )}

        {(actions.canRetry || actions.canApprove) && (
          <div className="grid gap-1 pt-1">
            {actions.canRetry && (
              <button
                type="button"
                disabled={actions.recoveryAction !== null}
                onClick={() => void actions.retry()}
                className={uiStyles.subtleButton}
              >
                {actions.recoveryAction === 'retry' ? 'Retrying…' : 'Retry task'}
              </button>
            )}
            {actions.canApprove && (
              <button
                type="button"
                disabled={actions.recoveryAction !== null}
                onClick={() => void actions.approve()}
                className={uiStyles.primaryButton}
              >
                {actions.recoveryAction === 'approve' ? 'Allowing…' : 'Allow and continue'}
              </button>
            )}
          </div>
        )}

        {actions.showActions && (
          <div className="grid gap-1 pt-1">
            {actions.confirmCancelTask ? (
              <>
                <p className="text-ui-caption leading-relaxed text-apple-red">
                  Canceling stops the current agent work. Use Needs help instead if you only need to
                  pause for missing input.
                </p>
                <button
                  type="button"
                  disabled={actions.taskAction !== null}
                  onClick={() => void actions.cancel()}
                  className={uiStyles.subtleButton}
                >
                  {actions.taskAction === 'cancel' ? 'Canceling…' : 'Cancel task'}
                </button>
                <button
                  type="button"
                  disabled={actions.taskAction !== null}
                  onClick={() => actions.setConfirmCancelTask(false)}
                  className={uiStyles.subtleButton}
                >
                  Keep running
                </button>
              </>
            ) : (
              <>
                <button
                  type="button"
                  disabled={actions.taskAction !== null}
                  onClick={() => void actions.pause()}
                  className={uiStyles.subtleButton}
                >
                  {actions.taskAction === 'block' ? 'Marking…' : 'Needs help'}
                </button>
                <button
                  type="button"
                  disabled={actions.taskAction !== null}
                  onClick={() => actions.setConfirmCancelTask(true)}
                  className={uiStyles.subtleButton}
                >
                  Cancel
                </button>
              </>
            )}
          </div>
        )}

        {actions.error && (
          <p role="alert" aria-live="polite" className="text-ui-caption text-apple-red">
            {actions.error}
          </p>
        )}
      </RailSection>

      <InjectionPreviewModal {...actions.previewState} />
    </>
  )
}
