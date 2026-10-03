import type { MaintenanceTrace } from '../../../shared/types/maintenance'

export const maintenanceTaskId = '11111111-1111-4111-8111-111111111111'
export const maintenanceRequestId = '22222222-2222-4222-8222-222222222222'
export const maintenanceGroupId = '33333333-3333-4333-8333-333333333333'
export function maintenanceTrace(): MaintenanceTrace {
  return {
    requestId: maintenanceRequestId,
    taskId: maintenanceTaskId,
    taskState: 'backlog',
    attempt: 0,
    repository: 'example-org/example-repo',
    defaultBranch: 'release/stable',
    startingSha: 'a'.repeat(40),
    createdAt: '2026-10-01T12:00:00Z',
    source: { kind: 'request', reference: 'dependency-2026-10', url: null, submittedHeadSha: null },
    executions: [],
    rebuildBaseSha: null,
    recordedPrHeadSha: null,
    producedPrUrl: null,
    sourceState: { status: 'not_applicable', checkedAt: null, snapshot: null, headChanged: false },
    producedState: { status: 'not_created', checkedAt: null, snapshot: null, headChanged: false },
  }
}
