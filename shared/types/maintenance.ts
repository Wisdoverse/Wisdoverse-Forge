/** Deliberate intake and trace contracts; mirrors Rust domain/maintenance.rs. */
export type MaintenanceSourceInput =
  { kind: 'request'; reference: string } | { kind: 'pull_request'; number: number }

export interface MaintenanceRequestInput {
  groupId: string
  title: string
  brief: string
  source: MaintenanceSourceInput
}

export interface MaintenanceSubmission {
  requestId: string
  taskId: string
  reused: boolean
}

export interface MaintenancePullRequest {
  number: number
  url: string
  state: 'open' | 'closed' | 'merged'
  headSha: string
  baseBranch: string
}

export interface MaintenanceObservation {
  status: 'observed' | 'unavailable' | 'not_applicable' | 'not_created'
  checkedAt: string | null
  snapshot: MaintenancePullRequest | null
  headChanged: boolean
}

export interface MaintenanceTrace {
  requestId: string
  taskId: string
  taskState: string
  attempt: number
  repository: string
  defaultBranch: string
  startingSha: string
  createdAt: string
  source: {
    kind: 'request' | 'pull_request'
    reference: string
    url: string | null
    submittedHeadSha: string | null
  }
  executions: {
    runId: string
    agentId: string
    state: string
    startedAt: string
    finishedAt: string | null
  }[]
  rebuildBaseSha: string | null
  recordedPrHeadSha: string | null
  producedPrUrl: string | null
  sourceState: MaintenanceObservation
  producedState: MaintenanceObservation
}
