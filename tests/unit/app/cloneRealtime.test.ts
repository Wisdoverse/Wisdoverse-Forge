import { beforeEach, describe, expect, it } from 'vitest'
import { dispatchWsMessage } from '@app/hooks/useWsDispatch'
import { parseCloneStatusUpdate } from '@app/features/manage-project/model/cloneRealtime'
import { useNavigationStore } from '@app/entities/navigation'
import type { NavProject } from '@app/entities/project'

function project(overrides: Partial<NavProject> = {}): NavProject {
  return {
    id: overrides.id ?? 'project-1',
    teamId: overrides.teamId ?? 'team-1',
    name: overrides.name ?? 'Web App',
    slug: overrides.slug ?? 'web-app',
    color: overrides.color ?? '#0066cc',
    description: overrides.description ?? '',
    cloneStatus: overrides.cloneStatus,
    clone: overrides.clone,
  }
}

function seedProjects(projects: NavProject[]) {
  useNavigationStore.setState({ projects: { 'team-1': projects } })
}

function currentProject(id = 'project-1'): NavProject | undefined {
  return Object.values(useNavigationStore.getState().projects)
    .flat()
    .find((p) => p.id === id)
}

function readyFrame(attempt: number, updatedAt: string) {
  return {
    type: 'project_clone:status_update' as const,
    payload: {
      action: 'clone.ready',
      eventId: `evt-${attempt}`,
      projectId: 'project-1',
      cloneStatus: 'ready',
      details: {
        project_id: 'project-1',
        attempt,
        updatedAt,
        branch: 'main',
        head_sha: 'abc1234deadbeef',
      },
    },
  }
}

/**
 * A clone frame at an explicit lifecycle status / attempt / timestamp, used to
 * exercise the same-attempt progression and status-only branches the dedicated
 * `readyFrame`/failure fixtures above do not reach.
 */
function frame(
  cloneStatus: 'queued' | 'cloning' | 'ready' | 'failed',
  attempt: number,
  updatedAt: string,
  details: Record<string, unknown> = {}
) {
  return {
    type: 'project_clone:status_update' as const,
    payload: {
      action: `clone.${cloneStatus}`,
      eventId: `evt-${cloneStatus}-${attempt}-${updatedAt}`,
      projectId: 'project-1',
      cloneStatus,
      details: {
        project_id: 'project-1',
        attempt,
        updatedAt,
        ...details,
      },
    },
  }
}

beforeEach(() => {
  useNavigationStore.getState().reset()
})

describe('parseCloneStatusUpdate', () => {
  it('decodes the worker frame (branch -> resolvedBranch, snake_case details)', () => {
    const update = parseCloneStatusUpdate(readyFrame(1, '2026-06-15T00:00:00.000Z'))
    expect(update).not.toBeNull()
    expect(update?.projectId).toBe('project-1')
    expect(update?.cloneStatus).toBe('ready')
    expect(update?.clone?.resolvedBranch).toBe('main')
    expect(update?.clone?.headSha).toBe('abc1234deadbeef')
    expect(update?.clone?.attempt).toBe(1)
  })

  it('returns null for a malformed frame (missing projectId / status)', () => {
    expect(
      parseCloneStatusUpdate({
        type: 'project_clone:status_update',
        payload: { details: { attempt: 1 } },
      })
    ).toBeNull()
    expect(
      parseCloneStatusUpdate({ type: 'project_clone:status_update', payload: null })
    ).toBeNull()
  })
})

describe('applyCloneStatusUpdate idempotency + monotonicity', () => {
  it('applies the same event twice and produces a single, stable state change', () => {
    seedProjects([project({ cloneStatus: 'cloning' })])
    const frame = readyFrame(1, '2026-06-15T00:00:00.000Z')

    dispatchWsMessage(frame)
    const afterFirst = currentProject()
    expect(afterFirst?.cloneStatus).toBe('ready')
    expect(afterFirst?.clone?.headSha).toBe('abc1234deadbeef')

    // Re-dispatching the identical frame must not change or regress state.
    dispatchWsMessage(frame)
    const afterSecond = currentProject()
    expect(afterSecond?.cloneStatus).toBe('ready')
    expect(afterSecond?.clone).toEqual(afterFirst?.clone)
  })

  it('never lets an older attempt regress a newer one (out-of-order delivery)', () => {
    seedProjects([project({ cloneStatus: 'cloning' })])

    // Newer attempt 2 arrives first, then a stale attempt 1 frame.
    dispatchWsMessage(readyFrame(2, '2026-06-15T00:02:00.000Z'))
    dispatchWsMessage({
      type: 'project_clone:status_update',
      payload: {
        action: 'clone.failed',
        eventId: 'evt-stale',
        projectId: 'project-1',
        cloneStatus: 'failed',
        details: {
          project_id: 'project-1',
          attempt: 1,
          updatedAt: '2026-06-15T00:01:00.000Z',
          error_class: 'auth',
          error_message: 'authentication failed',
        },
      },
    })

    // The stale attempt-1 failure must not overwrite the ready attempt-2 summary.
    const after = currentProject()
    expect(after?.clone?.attempt).toBe(2)
    expect(after?.cloneStatus).toBe('ready')
  })

  it('records a failed attempt with the redacted error message', () => {
    seedProjects([project({ cloneStatus: 'cloning' })])

    dispatchWsMessage({
      type: 'project_clone:status_update',
      payload: {
        action: 'clone.failed',
        eventId: 'evt-fail',
        projectId: 'project-1',
        cloneStatus: 'failed',
        details: {
          project_id: 'project-1',
          attempt: 1,
          updatedAt: '2026-06-15T00:00:30.000Z',
          error_class: 'not_found',
          error_message: 'repository not found',
        },
      },
    })

    const after = currentProject()
    expect(after?.cloneStatus).toBe('failed')
    expect(after?.clone?.errorMessage).toBe('repository not found')
    expect(after?.clone?.errorClass).toBe('not_found')
  })

  it('ignores a frame for an unknown project without throwing', () => {
    seedProjects([project()])
    expect(() =>
      dispatchWsMessage({
        type: 'project_clone:status_update',
        payload: {
          action: 'clone.ready',
          eventId: 'evt-x',
          projectId: 'project-unknown',
          cloneStatus: 'ready',
          details: { attempt: 1, updatedAt: '2026-06-15T00:00:00.000Z' },
        },
      })
    ).not.toThrow()
    expect(currentProject()?.cloneStatus).toBeUndefined()
  })

  it('advances queued -> cloning -> ready on a single attempt by newer updatedAt', () => {
    // The normal lifecycle on ONE attempt: every frame shares attempt 1, so the
    // store must fall through to the `updatedAt >=` comparison in
    // `isCloneUpdateNewer` (the equal-attempt branch the out-of-order test above
    // never reaches because it differs by attempt).
    seedProjects([
      project({
        cloneStatus: 'queued',
        clone: { status: 'queued', attempt: 1, updatedAt: '2026-06-15T00:00:00.000Z' },
      }),
    ])

    dispatchWsMessage(frame('cloning', 1, '2026-06-15T00:00:10.000Z'))
    expect(currentProject()?.cloneStatus).toBe('cloning')

    dispatchWsMessage(
      frame('ready', 1, '2026-06-15T00:00:20.000Z', {
        branch: 'main',
        head_sha: 'abc1234deadbeef',
      })
    )

    const after = currentProject()
    expect(after?.cloneStatus).toBe('ready')
    expect(after?.clone?.headSha).toBe('abc1234deadbeef')
    expect(after?.clone?.resolvedBranch).toBe('main')

    // A late same-attempt-1 frame with an OLDER timestamp must NOT regress
    // ready -> cloning. Guards a `>=`/`<=` swap or an attempt-only comparison.
    dispatchWsMessage(frame('cloning', 1, '2026-06-15T00:00:05.000Z'))
    const afterStale = currentProject()
    expect(afterStale?.cloneStatus).toBe('ready')
    expect(afterStale?.clone?.headSha).toBe('abc1234deadbeef')
  })

  it('keeps the prior summary when a status-only frame carries no attempt', () => {
    // A lean status-only broadcast (`details: {}`, so `cloneSummaryFromFrame`
    // returns undefined) must still advance the displayed status while
    // preserving the existing summary via `clone: clone ?? project.clone`.
    const priorClone = {
      status: 'ready' as const,
      attempt: 1,
      updatedAt: '2026-06-15T00:00:20.000Z',
      resolvedBranch: 'main',
      headSha: 'abc1234deadbeef',
    }
    seedProjects([project({ cloneStatus: 'ready', clone: priorClone })])
    const before = useNavigationStore.getState().projects

    dispatchWsMessage({
      type: 'project_clone:status_update',
      payload: {
        action: 'clone.cloning',
        eventId: 'evt-status-only',
        projectId: 'project-1',
        cloneStatus: 'cloning',
        details: {},
      },
    })

    const after = currentProject()
    expect(after?.cloneStatus).toBe('cloning')
    // The summary is untouched, not clobbered to undefined.
    expect(after?.clone).toEqual(priorClone)
    // The store actually committed the change (reducer set `changed = true`).
    expect(useNavigationStore.getState().projects).not.toBe(before)
  })
})
