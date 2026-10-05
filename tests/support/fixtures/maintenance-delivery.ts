import type {
  MaintenanceComparison,
  MaintenanceDelivery,
  MaintenanceOutcomes,
  ReviewDecision,
  VerificationReport,
} from '../../../shared/types/maintenance-delivery'
import { maintenanceTaskId, maintenanceTrace } from './maintenance'

const reportId = '44444444-4444-4444-8444-444444444444'
const runId = '55555555-5555-4555-8555-555555555555'
const agentId = '66666666-6666-4666-8666-666666666666'
const authorId = '77777777-7777-4777-8777-777777777777'
const reviewerId = '88888888-8888-4888-8888-888888888888'
const revision = 'b'.repeat(40)
const observedAt = '2026-10-01T12:05:00Z'

function traceWithRun() {
  const trace = maintenanceTrace()
  trace.taskState = 'completed'
  trace.executions = [
    {
      runId,
      agentId,
      state: 'completed',
      startedAt: '2026-10-01T12:01:00Z',
      finishedAt: '2026-10-01T12:04:00Z',
    },
  ]
  trace.recordedPrHeadSha = revision
  trace.producedPrUrl = 'https://github.com/example-org/example-repo/pull/42'
  trace.producedState = {
    status: 'observed',
    checkedAt: observedAt,
    snapshot: {
      number: 42,
      url: trace.producedPrUrl,
      state: 'open',
      headSha: revision,
      baseBranch: trace.defaultBranch,
    },
    headChanged: false,
  }
  return trace
}

export function maintenanceDeliveryReport(): VerificationReport {
  const trace = traceWithRun()
  return {
    id: reportId,
    taskId: maintenanceTaskId,
    runId,
    authorId,
    taskVersion: 3,
    revision,
    startingRevision: trace.startingSha,
    criteria: 'The change satisfies the stated acceptance criteria.',
    comparisonKey: 'shared-maintenance-comparison',
    createdAt: observedAt,
    snapshot: {
      title: 'Update dependency constraints',
      brief: 'Keep dependency resolution compatible with the stable release.',
      repository: trace.repository,
      revisionKind: 'produced',
      scope: 'Dependency metadata only.',
      environmentNotes: 'Disposable test environment.',
      cliVersion: '1.0.0',
      changeSummary: 'Updated dependency constraints and verified the result.',
      reportedChecks: [
        {
          name: 'Unit tests',
          command: 'npm run test:unit',
          status: 'passed',
          evidence: 'All targeted tests passed.',
        },
      ],
      unverified: [],
      noArtifactReason: null,
      runtime: {
        runId,
        agentId,
        state: 'completed',
        startedAt: '2026-10-01T12:01:00Z',
        finishedAt: '2026-10-01T12:04:00Z',
        runtimeKind: 'container',
        cliTool: 'codex',
        providerName: 'test-provider',
        image: null,
      },
      evidence: [],
      evidenceComplete: true,
      github: {
        status: 'observed',
        checkedAt: observedAt,
        revision,
        complete: true,
        checks: [
          {
            kind: 'check_run',
            name: 'CI',
            state: 'completed',
            conclusion: 'success',
          },
        ],
      },
    },
  }
}

export function maintenanceDeliveryDecision(): ReviewDecision {
  return {
    id: '99999999-9999-4999-8999-999999999999',
    reportId,
    reviewerId,
    verdict: 'accepted',
    reason: 'Reviewed the report and accepted the result.',
    humanMinutes: {
      setup: 0,
      handling: 0,
      review: 1,
      recovery: 0,
      rework: 0,
      operation: 0,
    },
    totalMinutes: 1,
    github: maintenanceDeliveryReport().snapshot.github,
    baselineMinutes: null,
    createdAt: observedAt,
  }
}

export function maintenanceDelivery(): MaintenanceDelivery {
  const trace = traceWithRun()
  return {
    trace,
    taskVersion: 3,
    currentRevision: revision,
    latestRunId: runId,
    recovery: {
      observedAt,
      taskState: 'completed',
      attempt: 1,
      failureCode: null,
      blockedReason: null,
      manualRetryAllowed: false,
      mergeAttempts: 0,
      mergeLimit: 3,
      reviewStatus: 'in_review',
      bridge: null,
    },
    currentChecks: {
      status: 'observed',
      checkedAt: observedAt,
      revision,
      complete: true,
      checks: [{ kind: 'check_run', name: 'CI', state: 'completed', conclusion: 'success' }],
    },
    reports: [maintenanceDeliveryReport()],
    decisions: [maintenanceDeliveryDecision()],
    handoffs: [],
    hasMoreReports: false,
    hasMoreDecisions: false,
    hasMoreHandoffs: false,
  }
}

export function maintenanceDeliveryOutcomes(): MaintenanceOutcomes {
  const report = maintenanceDeliveryReport()
  const decision = maintenanceDeliveryDecision()
  return {
    from: '2026-10-01T00:00:00Z',
    to: observedAt,
    projectId: null,
    observedAt,
    summary: {
      submitted: 1,
      accepted: 1,
      rejected: 0,
      rework: 0,
      reopened: 0,
      awaitingReview: 0,
      staleReviews: 0,
      failed: 0,
      canceled: 0,
      completeEffortTasks: 1,
      humanMinutes: 1,
      pairedBaselineTasks: 0,
      pairedHumanMinutes: null,
      baselineMinutes: null,
      acceptedInPeriod: 1,
      reopenedInPeriod: 0,
    },
    tasks: [
      {
        taskId: maintenanceTaskId,
        title: report.snapshot.title,
        state: 'completed',
        createdAt: '2026-10-01T12:00:00Z',
        report,
        decision,
        reviewCurrent: true,
        leadTimeMinutes: 5,
      },
    ],
    nextCursor: null,
  }
}

export function maintenanceDeliveryComparison(): MaintenanceComparison {
  const report = maintenanceDeliveryReport()
  return {
    conditionsMatch: true,
    distinctClis: 2,
    reasons: [],
    rows: [
      { report, decision: maintenanceDeliveryDecision() },
      {
        report: {
          ...report,
          id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
          runId: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
          snapshot: {
            ...report.snapshot,
            runtime: report.snapshot.runtime && {
              ...report.snapshot.runtime,
              runId: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
              cliTool: 'claude',
            },
          },
        },
        decision: null,
      },
    ],
  }
}
