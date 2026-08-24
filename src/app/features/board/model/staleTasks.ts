import type { TaskSummary } from '@app/shared/api/orchestration'

/** Default retirement window: tasks untouched this long count as stale. */
export const STALE_RETIRE_DAYS = 7

/**
 * A task is stale when it was never started (backlog/queued, 0% progress) and
 * has not changed for `days`. Terminal, working, and in-progress tasks are
 * never stale, and an unparseable timestamp is treated as fresh (report it
 * rather than risk closing active work).
 */
export function isStaleTask(
  task: Pick<TaskSummary, 'state' | 'progress' | 'updatedAt' | 'createdAt'>,
  days = STALE_RETIRE_DAYS
): boolean {
  if (task.state !== 'backlog' && task.state !== 'queued') return false
  if ((task.progress ?? 0) !== 0) return false
  const stamp = task.updatedAt || task.createdAt
  const time = Date.parse(stamp)
  if (Number.isNaN(time)) return false
  return time <= Date.now() - days * 86_400_000
}

/** How many of the given tasks are stale (the reported retire count). */
export function staleTaskCount(tasks: TaskSummary[]): number {
  return tasks.filter((task) => isStaleTask(task)).length
}
