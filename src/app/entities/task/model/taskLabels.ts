import type { TaskSummary } from '@app/shared/api/orchestration'

const TASK_STATE_LABELS: Record<string, string> = {
  backlog: 'Not sent yet',
  queued: 'Waiting to start',
  working: 'Working',
  blocked: 'Needs help',
  completed: 'Completed',
  failed: 'Needs review',
  canceled: 'Canceled',
}

const TASK_PRIORITY_LABELS: Record<string, string> = {
  urgent: 'Urgent',
  high: 'High',
  normal: 'Normal',
  low: 'Low',
}

interface TaskStateLabelOptions {
  completedLabel?: string
}

export function taskStateLabel(
  state: TaskSummary['state'] | string | null | undefined,
  options: TaskStateLabelOptions = {}
): string {
  const normalized = normalizedMachineValue(state)
  if (!normalized) return 'Refresh task status'
  if (normalized === 'completed' && options.completedLabel) return options.completedLabel
  return TASK_STATE_LABELS[normalized] ?? 'Check task status'
}

export function taskPriorityLabel(
  priority: TaskSummary['priority'] | string | null | undefined
): string {
  const normalized = normalizedMachineValue(priority)
  if (!normalized) return 'Refresh task priority'
  return TASK_PRIORITY_LABELS[normalized] ?? 'Check task priority'
}

export function taskMachineKey(value: string | null | undefined): string {
  return normalizedMachineValue(value)
}

function normalizedMachineValue(value: string | null | undefined): string {
  return typeof value === 'string' ? value.trim().toLowerCase() : ''
}
