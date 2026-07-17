import type { TaskSummary } from '@app/shared/api/orchestration'
import { taskBlockedPreview, taskFailurePreview } from '@app/shared/lib/taskFailureCopy'
import { TASK_AGENT_NAME_LOADING_LABEL } from './taskAgentLabels'

export const HANDOFF_REVIEW_POINTS = [
  { label: 'Outcome', value: 'Confirm the result solves the original request.' },
  { label: 'Check work', value: 'Open result files or what the agent used before accepting.' },
  {
    label: 'Reuse',
    value: 'Save the repeatable steps only when they should help future tasks.',
  },
]

export function taskHasBrief(task: TaskSummary): boolean {
  return task.params.message?.trim().length > 0
}

export function missingBriefCopy(task: TaskSummary): string {
  if (task.state === 'backlog') {
    return 'Only the task title was saved. Before sending, add what to finish, where to look, and how you will check it.'
  }
  return 'No brief was saved. Open Updates to see what was asked before accepting, retrying, or closing this task.'
}

export function assignmentSummary(task: TaskSummary): {
  label: string
  detail: string
  hasAgent: boolean
} {
  if (task.assignedAgentName) {
    return {
      label: task.assignedAgentName,
      detail: assignedAgentDetail(task),
      hasAgent: true,
    }
  }
  if (task.assignedTo) {
    return {
      label: TASK_AGENT_NAME_LOADING_LABEL,
      detail: assignedAgentLoadingDetail(task),
      hasAgent: true,
    }
  }
  return {
    label: 'Needs agent',
    detail: 'Choose an agent before this task can start.',
    hasAgent: false,
  }
}

function assignedAgentDetail(task: TaskSummary): string {
  switch (task.state) {
    case 'working':
      return 'This agent is working on this task now.'
    case 'blocked':
      return 'This agent needs your answer before it can continue.'
    case 'completed':
      return 'This agent finished this task. Check the result before accepting it.'
    case 'failed':
      return 'This agent tried this task. Check retry steps before trying again.'
    case 'canceled':
      return 'This agent was chosen before the task stopped.'
    default:
      return 'This agent will handle the next step for this task.'
  }
}

function assignedAgentLoadingDetail(task: TaskSummary): string {
  switch (task.state) {
    case 'working':
      return 'An agent is working on this task, but its name has not loaded yet. Open this task again so you can confirm the right agent.'
    case 'completed':
      return 'An agent finished this task, but its name has not loaded yet. Open this task again so you can confirm who handled it.'
    case 'failed':
      return 'An agent tried this task, but its name has not loaded yet. Open this task again so you can confirm who to retry with.'
    default:
      return 'An agent was chosen, but its name has not loaded yet. Open this task again so you can confirm the right agent before sending it.'
  }
}

export function nextActionForTask(
  task: TaskSummary,
  artifactCount: number,
  contextTotal: number
): { title: string; detail: string; tone: 'default' | 'success' | 'warn' } {
  const hasBrief = taskHasBrief(task)
  const hasAgent = Boolean(task.assignedTo || task.assignedAgentName)

  switch (task.state) {
    case 'backlog':
      if (!hasBrief) {
        return hasAgent
          ? {
              title: 'Add details before sending',
              detail:
                'This task only has a title. Add what to finish, where to look, and how you will check it before sending.',
              tone: 'warn',
            }
          : {
              title: 'Add details and choose an agent',
              detail:
                'This task only has a title. Add what to finish, where to look, and how to check it, then choose an agent.',
              tone: 'warn',
            }
      }
      return hasAgent
        ? {
            title: 'Ready to send',
            detail: 'Check the brief, then send it to this agent.',
            tone: 'default',
          }
        : {
            title: 'Assign an agent',
            detail:
              'Choose an agent, check the suggested saved notes and guidance, then send the task.',
            tone: 'warn',
          }
    case 'queued':
      return task.assignedTo || task.assignedAgentName
        ? {
            title: 'Waiting for the agent to start',
            detail:
              'If this stays here, open Updates to check the last activity, then choose another agent if needed.',
            tone: 'default',
          }
        : {
            title: 'Waiting for an agent',
            detail:
              'If this stays here, choose or start an agent so the task has someone to begin the work.',
            tone: 'warn',
          }
    case 'working':
      return {
        title: 'Monitor progress',
        detail:
          task.progress >= 80
            ? 'Prepare to check result files when the agent finishes this task.'
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
        title: 'Check the handoff',
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
        title: 'Check retry steps',
        detail: taskFailurePreview(task.error),
        tone: 'warn',
      }
    case 'canceled':
      return {
        title: 'Decide whether to continue',
        detail: 'Create a new task or reopen the brief if this work still matters.',
        tone: 'default',
      }
    default:
      return {
        title: 'Check current status',
        detail: 'Open Updates to check the latest activity before starting, retrying, or closing.',
        tone: 'warn',
      }
  }
}
