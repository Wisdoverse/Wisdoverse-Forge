import type * as T from '@shared/types/maintenance-delivery'
import { MaintenanceError, trace } from './api'

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
const SHA = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/
const REPO = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/
function invalid(): never {
  throw new MaintenanceError('invalid-response')
}
function obj(v: unknown): Record<string, unknown> {
  return v !== null && typeof v === 'object' && !Array.isArray(v)
    ? (v as Record<string, unknown>)
    : invalid()
}
function str(v: unknown, max = 8000, pattern?: RegExp): string {
  return typeof v === 'string' && v.length > 0 && v.length <= max && (!pattern || pattern.test(v))
    ? v
    : invalid()
}
function id(v: unknown) {
  return str(v, 36, UUID)
}
function sha(v: unknown) {
  return str(v, 64, SHA)
}
function int(v: unknown, max = Number.MAX_SAFE_INTEGER): number {
  return typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= max ? v : invalid()
}
function bool(v: unknown): boolean {
  return typeof v === 'boolean' ? v : invalid()
}
function time(v: unknown): string {
  const s = str(v, 64)
  return Number.isFinite(Date.parse(s)) ? s : invalid()
}
function nullable<V>(v: unknown, read: (v: unknown) => V): V | null {
  return v === null ? null : read(v)
}
function list<V>(v: unknown, limit: number, read: (v: unknown) => V): V[] {
  return Array.isArray(v) && v.length <= limit ? v.map(read) : invalid()
}
function choice<V extends string>(v: unknown, values: readonly V[]): V {
  return typeof v === 'string' && values.includes(v as V) ? (v as V) : invalid()
}
function unique<V>(items: V[], key: (v: V) => string): V[] {
  return new Set(items.map(key)).size === items.length ? items : invalid()
}
export function checks(v: unknown): T.GithubVerification {
  const d = obj(v)
  const status = choice(d.status, [
    'observed',
    'unavailable',
    'not_created',
    'head_changed',
  ] as const)
  const result = {
    status,
    checkedAt: nullable(d.checkedAt, time),
    revision: nullable(d.revision, sha),
    complete: bool(d.complete),
    checks: list(d.checks, 1000, (v) => {
      const c = obj(v)
      return {
        kind: choice(c.kind, ['check_run', 'commit_status'] as const),
        name: str(c.name, 1024),
        state: str(c.state, 256),
        conclusion: nullable(c.conclusion, (v) => str(v, 256)),
      }
    }),
  }
  if (
    (status === 'observed' || status === 'head_changed') &&
    (!result.checkedAt || !result.revision)
  )
    invalid()
  if (status !== 'observed' && result.checks.length) invalid()
  return result
}
function runtime(v: unknown): T.RuntimeEvidence {
  const d = obj(v)
  return {
    runId: id(d.runId),
    agentId: id(d.agentId),
    state: str(d.state, 64),
    startedAt: time(d.startedAt),
    finishedAt: nullable(d.finishedAt, time),
    runtimeKind: nullable(d.runtimeKind, (v) => str(v, 64)),
    cliTool: nullable(d.cliTool, (v) => str(v, 64)),
    providerName: nullable(d.providerName, (v) => str(v, 256)),
    image: nullable(d.image, (v) => {
      const i = obj(v)
      return {
        source: str(i.source, 256),
        imageId: str(i.imageId, 1024),
        manifestDigest: nullable(i.manifestDigest, (v) => str(v, 256)),
        version: nullable(i.version, (v) => str(v, 256)),
        versionSource: str(i.versionSource, 256),
        trust: nullable(i.trust, (v) => str(v, 256)),
      }
    }),
  }
}
export function report(v: unknown): T.VerificationReport {
  const d = obj(v),
    s = obj(d.snapshot)
  const snapshot: T.ReportSnapshot = {
    title: str(s.title, 4000),
    brief: nullable(s.brief, (v) => str(v, 16000)),
    repository: str(s.repository, 512, REPO),
    revisionKind: choice(s.revisionKind, ['produced', 'starting'] as const),
    scope: str(s.scope, 4000),
    environmentNotes: str(s.environmentNotes, 4000),
    cliVersion: nullable(s.cliVersion, (v) => str(v, 256)),
    changeSummary: str(s.changeSummary, 8000),
    reportedChecks: list(s.reportedChecks, 20, (v) => {
      const c = obj(v)
      return {
        name: str(c.name, 256),
        command: str(c.command, 1000),
        status: choice(c.status, ['passed', 'failed', 'not_run'] as const),
        evidence: str(c.evidence, 2000),
      }
    }),
    unverified: list(s.unverified, 20, (v) => str(v, 1000)),
    noArtifactReason: nullable(s.noArtifactReason, (v) => str(v, 2000)),
    runtime: nullable(s.runtime, runtime),
    evidence: list(s.evidence, 100, (v) => {
      const e = obj(v)
      return {
        sourceType: str(e.sourceType, 256),
        sourceId: id(e.sourceId),
        createdAt: time(e.createdAt),
      }
    }),
    evidenceComplete: bool(s.evidenceComplete),
    github: checks(s.github),
  }
  const r = {
    id: id(d.id),
    taskId: id(d.taskId),
    runId: nullable(d.runId, id),
    authorId: id(d.authorId),
    taskVersion: int(d.taskVersion),
    revision: sha(d.revision),
    startingRevision: sha(d.startingRevision),
    criteria: str(d.criteria, 4000),
    comparisonKey: nullable(d.comparisonKey, (v) => str(v, 64, /^[a-z0-9][a-z0-9._:-]*$/)),
    createdAt: time(d.createdAt),
    snapshot,
  }
  if ((snapshot.runtime?.runId ?? null) !== r.runId) invalid()
  return r
}
export function decision(v: unknown): T.ReviewDecision {
  const d = obj(v),
    m = obj(d.humanMinutes)
  const minute = (v: unknown) => nullable(v, (v) => int(v, 100000))
  const humanMinutes = {
    setup: minute(m.setup),
    handling: minute(m.handling),
    review: minute(m.review),
    recovery: minute(m.recovery),
    rework: minute(m.rework),
    operation: minute(m.operation),
  }
  const totalMinutes = nullable(d.totalMinutes, (v) => int(v, 600000))
  const values = Object.values(humanMinutes)
  if (
    totalMinutes !==
    (values.includes(null) ? null : values.reduce<number>((a, b) => a + (b ?? 0), 0))
  )
    invalid()
  return {
    id: id(d.id),
    reportId: id(d.reportId),
    reviewerId: id(d.reviewerId),
    verdict: choice(d.verdict, ['accepted', 'rejected', 'rework', 'reopened'] as const),
    reason: str(d.reason, 4000),
    humanMinutes,
    totalMinutes,
    github: nullable(d.github, checks),
    baselineMinutes: nullable(d.baselineMinutes, (v) => int(v, 600000)),
    createdAt: time(d.createdAt),
  }
}
function recovery(v: unknown): T.RecoverySnapshot {
  const d = obj(v)
  return {
    observedAt: time(d.observedAt),
    taskState: str(d.taskState, 64),
    attempt: int(d.attempt),
    failureCode: nullable(d.failureCode, (v) => str(v, 256)),
    blockedReason: nullable(d.blockedReason, (v) => str(v, 256)),
    manualRetryAllowed: bool(d.manualRetryAllowed),
    mergeAttempts: int(d.mergeAttempts),
    mergeLimit: int(d.mergeLimit),
    reviewStatus: nullable(d.reviewStatus, (v) => str(v, 64)),
    bridge: nullable(d.bridge, (v) => {
      const b = obj(v)
      return {
        state: str(b.state, 64),
        attempts: int(b.attempts),
        limit: int(b.limit),
        nextAttemptAt: nullable(b.nextAttemptAt, time),
      }
    }),
  }
}
export function handoff(v: unknown): T.MaintenanceHandoff {
  const d = obj(v),
    s = obj(d.snapshot)
  return {
    id: id(d.id),
    taskId: id(d.taskId),
    authorId: id(d.authorId),
    reason: str(d.reason, 4000),
    nextStep: str(d.nextStep, 4000),
    createdAt: time(d.createdAt),
    snapshot: {
      observedAt: time(s.observedAt),
      revision: sha(s.revision),
      runId: nullable(s.runId, id),
      taskVersion: int(s.taskVersion),
      recovery: recovery(s.recovery),
      reportIds: list(s.reportIds, 20, id),
    },
  }
}
export function delivery(v: unknown, taskId: string): T.MaintenanceDelivery | null {
  if (v === null) return null
  const d = obj(v),
    lineage = trace(d.trace, taskId)
  if (!lineage) invalid()
  const reports = unique(list(d.reports, 20, report), (r) => r.id)
  const handoffs = unique(list(d.handoffs, 20, handoff), (h) => h.id)
  if (handoffs.some((h) => h.taskId !== taskId)) invalid()
  if (reports.some((r) => r.taskId !== taskId || r.snapshot.repository !== lineage.repository))
    invalid()
  return {
    trace: lineage,
    taskVersion: int(d.taskVersion),
    currentRevision: sha(d.currentRevision),
    latestRunId: nullable(d.latestRunId, id),
    recovery: recovery(d.recovery),
    currentChecks: checks(d.currentChecks),
    reports,
    decisions: unique(list(d.decisions, 100, decision), (d) => d.id),
    handoffs,
    hasMoreReports: bool(d.hasMoreReports),
    hasMoreDecisions: bool(d.hasMoreDecisions),
    hasMoreHandoffs: bool(d.hasMoreHandoffs),
  }
}

export function outcomes(v: unknown): T.MaintenanceOutcomes {
  const d = obj(v),
    s = obj(d.summary)
  const summary: T.OutcomeSummary = {
    submitted: int(s.submitted),
    accepted: int(s.accepted),
    rejected: int(s.rejected),
    rework: int(s.rework),
    reopened: int(s.reopened),
    awaitingReview: int(s.awaitingReview),
    staleReviews: int(s.staleReviews),
    failed: int(s.failed),
    canceled: int(s.canceled),
    completeEffortTasks: int(s.completeEffortTasks),
    humanMinutes: nullable(s.humanMinutes, int),
    pairedBaselineTasks: int(s.pairedBaselineTasks),
    pairedHumanMinutes: nullable(s.pairedHumanMinutes, int),
    baselineMinutes: nullable(s.baselineMinutes, int),
    acceptedInPeriod: int(s.acceptedInPeriod),
    reopenedInPeriod: int(s.reopenedInPeriod),
  }
  if (
    summary.accepted +
      summary.rejected +
      summary.rework +
      summary.reopened +
      summary.awaitingReview +
      summary.staleReviews !==
      summary.submitted ||
    summary.completeEffortTasks > summary.submitted ||
    summary.pairedBaselineTasks > summary.completeEffortTasks
  )
    invalid()
  const tasks = unique(
    list(d.tasks, 100, (v) => {
      const r = obj(v)
      const taskId = id(r.taskId),
        saved = nullable(r.report, report),
        verdict = nullable(r.decision, decision)
      if ((saved && saved.taskId !== taskId) || (verdict && verdict.reportId !== saved?.id))
        invalid()
      return {
        taskId,
        title: str(r.title, 4000),
        state: str(r.state, 64),
        createdAt: time(r.createdAt),
        report: saved,
        decision: verdict,
        reviewCurrent: bool(r.reviewCurrent),
        leadTimeMinutes: nullable(r.leadTimeMinutes, int),
      }
    }),
    (t) => t.taskId
  )
  return {
    from: time(d.from),
    to: time(d.to),
    projectId: nullable(d.projectId, id),
    observedAt: time(d.observedAt),
    summary,
    tasks,
    nextCursor: nullable(d.nextCursor, id),
  }
}
export function comparison(v: unknown, ids: string[]): T.MaintenanceComparison {
  const d = obj(v),
    rows = unique(
      list(d.rows, 8, (v) => {
        const row = obj(v)
        const saved = report(row.report),
          verdict = nullable(row.decision, decision)
        if (verdict && verdict.reportId !== saved.id) invalid()
        return { report: saved, decision: verdict }
      }),
      (r) => r.report.id
    )
  if (rows.length !== ids.length || rows.some((r) => !ids.includes(r.report.id))) invalid()
  return {
    conditionsMatch: bool(d.conditionsMatch),
    distinctClis: int(d.distinctClis, 8),
    reasons: list(d.reasons, 20, (v) => str(v, 2000)),
    rows,
  }
}
export function taskIdentity(taskId: string): void {
  id(taskId)
}
