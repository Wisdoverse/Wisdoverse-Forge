/** Contracts mirrored from rust/crates/api/src/domain/maintenance_delivery.rs. */
import type { MaintenanceTrace } from './maintenance'

export type ReportedCheckStatus = 'passed' | 'failed' | 'not_run'

export interface ReportedCheck {
  name: string
  command: string
  status: ReportedCheckStatus
  evidence: string
}

export interface VerificationInput {
  requestKey: string
  expectedVersion: number
  expectedRevision: string
  runId?: string | null
  criteria: string
  scope: string
  environmentNotes: string
  cliVersion?: string | null
  changeSummary: string
  checks: ReportedCheck[]
  unverified: string[]
  noArtifactReason?: string | null
  comparisonKey?: string | null
}

export type HumanVerdict = 'accepted' | 'rejected' | 'rework' | 'reopened'

/** Input representation allows omitted nullable minute categories. */
export interface HumanMinutesInput {
  setup?: number | null
  handling?: number | null
  review?: number | null
  recovery?: number | null
  rework?: number | null
  operation?: number | null
}

/** Serialized HumanMinutes always includes every nullable category. */
export interface HumanMinutes {
  setup: number | null
  handling: number | null
  review: number | null
  recovery: number | null
  rework: number | null
  operation: number | null
}

export interface DecisionInput {
  requestKey: string
  reportId: string
  expectedVersion: number
  expectedRevision: string
  verdict: HumanVerdict
  reason: string
  humanMinutes: HumanMinutesInput
  baselineMinutes?: number | null
}

export interface HandoffInput {
  requestKey: string
  expectedVersion: number
  expectedRevision: string
  reason: string
  nextStep: string
}

export interface ObservedCheck {
  kind: string
  name: string
  state: string
  conclusion: string | null
}

export interface GithubVerification {
  status: 'observed' | 'unavailable' | 'not_created' | 'head_changed'
  checkedAt: string | null
  revision: string | null
  complete: boolean
  checks: ObservedCheck[]
}

/** Mirrors orchestration::TaskRunImageSummary; no equivalent shared TS type exists. */
export interface TaskRunImageSummary {
  source: string
  imageId: string
  manifestDigest: string | null
  version: string | null
  versionSource: string
  trust: string | null
}

export interface RuntimeEvidence {
  runId: string
  agentId: string
  state: string
  startedAt: string
  finishedAt: string | null
  runtimeKind: string | null
  cliTool: string | null
  providerName: string | null
  image: TaskRunImageSummary | null
}

export interface EvidenceReference {
  sourceType: string
  sourceId: string
  createdAt: string
}

export interface ReportSnapshot {
  title: string
  brief: string | null
  repository: string
  revisionKind: 'produced' | 'starting'
  scope: string
  environmentNotes: string
  cliVersion: string | null
  changeSummary: string
  reportedChecks: ReportedCheck[]
  unverified: string[]
  noArtifactReason: string | null
  runtime: RuntimeEvidence | null
  evidence: EvidenceReference[]
  evidenceComplete: boolean
  github: GithubVerification
}

export interface VerificationReport {
  id: string
  taskId: string
  runId: string | null
  authorId: string
  taskVersion: number
  revision: string
  startingRevision: string
  criteria: string
  comparisonKey: string | null
  createdAt: string
  snapshot: ReportSnapshot
}

export interface ReviewDecision {
  id: string
  reportId: string
  reviewerId: string
  verdict: HumanVerdict
  reason: string
  humanMinutes: HumanMinutes
  totalMinutes: number | null
  github: GithubVerification | null
  baselineMinutes: number | null
  createdAt: string
}

export interface BridgeRetry {
  state: string
  attempts: number
  limit: number
  nextAttemptAt: string | null
}

export interface RecoverySnapshot {
  observedAt: string
  taskState: string
  attempt: number
  failureCode: string | null
  blockedReason: string | null
  manualRetryAllowed: boolean
  mergeAttempts: number
  mergeLimit: number
  reviewStatus: string | null
  bridge: BridgeRetry | null
}

export interface HandoffSnapshot {
  observedAt: string
  revision: string
  runId: string | null
  taskVersion: number
  recovery: RecoverySnapshot
  reportIds: string[]
}

export interface MaintenanceHandoff {
  id: string
  taskId: string
  authorId: string
  reason: string
  nextStep: string
  createdAt: string
  snapshot: HandoffSnapshot
}

export interface MaintenanceDelivery {
  trace: MaintenanceTrace
  taskVersion: number
  currentRevision: string
  latestRunId: string | null
  recovery: RecoverySnapshot
  currentChecks: GithubVerification
  reports: VerificationReport[]
  decisions: ReviewDecision[]
  handoffs: MaintenanceHandoff[]
  hasMoreReports: boolean
  hasMoreDecisions: boolean
  hasMoreHandoffs: boolean
}

export interface OutcomeQuery {
  from?: string | null
  to?: string | null
  projectId?: string | null
  cursor?: string | null
}

export interface OutcomeSummary {
  submitted: number
  accepted: number
  rejected: number
  rework: number
  reopened: number
  awaitingReview: number
  staleReviews: number
  failed: number
  canceled: number
  completeEffortTasks: number
  humanMinutes: number | null
  pairedBaselineTasks: number
  pairedHumanMinutes: number | null
  baselineMinutes: number | null
  acceptedInPeriod: number
  reopenedInPeriod: number
}

export interface OutcomeTask {
  taskId: string
  title: string
  state: string
  createdAt: string
  report: VerificationReport | null
  decision: ReviewDecision | null
  reviewCurrent: boolean
  leadTimeMinutes: number | null
}

export interface MaintenanceOutcomes {
  from: string
  to: string
  projectId: string | null
  observedAt: string
  summary: OutcomeSummary
  tasks: OutcomeTask[]
  nextCursor: string | null
}

export interface ComparisonQuery {
  reportIds: string
}

export interface ComparisonRow {
  report: VerificationReport
  decision: ReviewDecision | null
}

export interface MaintenanceComparison {
  conditionsMatch: boolean
  distinctClis: number
  reasons: string[]
  rows: ComparisonRow[]
}
