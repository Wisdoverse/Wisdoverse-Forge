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
})
