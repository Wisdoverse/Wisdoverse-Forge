import { taskResultArtifacts, type TaskSummary } from '@app/shared/api/orchestration'

export interface GettingStartedTaskSnapshot {
  total: number
  assigned: number
  completed: number
  artifacts: number
  appliedSkills: number
}

export interface GettingStartedProgressInput {
  hasWorkspace: boolean
  runtimeReady: boolean
  executionCredentialReady: boolean
  hasAgent: boolean
  hasRouting: boolean
  taskSnapshot: GettingStartedTaskSnapshot
  hasReusableLearning: boolean
}

export function summarizeGettingStartedTasks(tasks: TaskSummary[]): GettingStartedTaskSnapshot {
  const byId = new Map(tasks.map((task) => [task.id, task]))
  const snapshot: GettingStartedTaskSnapshot = {
    total: byId.size,
    assigned: 0,
    completed: 0,
    artifacts: 0,
    appliedSkills: 0,
  }

  for (const task of byId.values()) {
    if (task.assignedTo || task.assignedAgentName) snapshot.assigned += 1
    if (task.state === 'completed') snapshot.completed += 1
    snapshot.artifacts += taskResultArtifacts(task.result).length
    snapshot.appliedSkills += task.contextCounts?.appliedSkills ?? 0
  }

  return snapshot
}

export function getGettingStartedProgress(input: GettingStartedProgressInput) {
  const completion = {
    workspace: input.hasWorkspace,
    runtime: input.runtimeReady,
    provider: input.executionCredentialReady,
    agent: input.hasAgent,
    routing: input.hasRouting,
    task: input.taskSnapshot.total > 0,
    review: input.taskSnapshot.completed > 0 || input.taskSnapshot.artifacts > 0,
    reuse: input.hasReusableLearning,
  }

  return {
    completion,
    completeCount: Object.values(completion).filter(Boolean).length,
    total: Object.keys(completion).length,
  }
}
