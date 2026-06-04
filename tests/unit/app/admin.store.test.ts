import { beforeEach, describe, expect, test, vi } from 'vitest'

const authFetchMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/legacy', () => ({
  getAuthFetch: () => authFetchMock,
}))

import { adminHttpErrorMessage, useAdminStore } from '@app/shared/model/admin.store'

function response(status: number, body: unknown): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  } as Response
}

function resetAdminState() {
  useAdminStore.setState({
    activeSection: 'users',
    users: [],
    usersTotal: 0,
    usersPage: 1,
    usersLoading: false,
    usersError: null,
    userSearch: '',
    orgs: [],
    orgsLoading: false,
    orgsError: null,
    health: null,
    healthLoading: false,
    healthError: null,
    cliImages: null,
    cliImagesLoading: false,
    cliImagesError: null,
    cliImageRollingTool: null,
    cliImageRollResult: null,
    cliImageRollError: null,
  })
}

function expectBeginnerError(actual: string | null, expected: string): void {
  expect(actual).toBe(expected)
  expect(actual).not.toContain('Code:')
  expect(actual).not.toContain('Details:')
}

beforeEach(() => {
  resetAdminState()
  authFetchMock.mockReset()
})

describe('adminHttpErrorMessage', () => {
  test('turns expired admin auth into a sign-in step', () => {
    expectBeginnerError(
      adminHttpErrorMessage('users', 401),
      'Sign in again, then open Admin and reload the user list.'
    )
  })

  test('turns admin permission failures into an owner role step', () => {
    expectBeginnerError(
      adminHttpErrorMessage('organizations', 403),
      'You do not have permission to view admin organization list. Ask an owner to update your admin role.'
    )
  })

  test('turns server failures into an owner recovery step', () => {
    expectBeginnerError(
      adminHttpErrorMessage('health', 503, { error: { message: 'database down' } }),
      'The admin service is temporarily unavailable. Reload the system health, then try again. If it still fails, ask an owner to check the admin service.'
    )
  })
})

describe('useAdminStore loading errors', () => {
  test('stores beginner guidance when user loading is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'owner role required' }))

    await useAdminStore.getState().loadUsers()

    expectBeginnerError(
      useAdminStore.getState().usersError,
      'You do not have permission to view admin user list. Ask an owner to update your admin role.'
    )
  })

  test('stores a connection recovery step when organization loading cannot reach the server', async () => {
    authFetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await useAdminStore.getState().loadOrgs()

    expect(useAdminStore.getState().orgsError).toBe(
      'The admin organization list could not load because the app could not reach the service. Check your connection and refresh the page.'
    )
  })

  test('returns beginner guidance when user access saving is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'owner role required' }))

    const result = await useAdminStore.getState().updateUserRole('user-1', 'viewer')

    expect(result).toEqual({
      ok: false,
      error:
        'You do not have permission to change user access. Ask an owner to update your admin role.',
    })
    expect(result.error).not.toContain('Code:')
    expect(result.error).not.toContain('Details:')
  })

  test('stores service recovery guidance when health loading fails', async () => {
    authFetchMock.mockResolvedValue(response(503, { message: 'health database unavailable' }))

    await useAdminStore.getState().loadHealth()

    expectBeginnerError(
      useAdminStore.getState().healthError,
      'The admin service is temporarily unavailable. Reload the system health, then try again. If it still fails, ask an owner to check the admin service.'
    )
  })

  test('loads the CLI image status report on success', async () => {
    authFetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        data: {
          autoUpdateEnabled: false,
          pollIntervalSecs: 900,
          registry: 'ghcr.io/wisdoverse/wisdoverse-forge',
          imageTag: 'latest',
          tools: [
            {
              tool: 'codex',
              state: 'pending',
              localDigest: null,
              remoteDigest: null,
              lastCheckedUnix: null,
              lastUpdatedUnix: null,
              lastError: null,
              agentsWithContainer: 0,
            },
          ],
          prune: {
            enabled: false,
            lastRunUnix: null,
            scanned: 0,
            removed: 0,
            skippedInUse: 0,
            skippedConflict: 0,
            errors: 0,
            lastError: null,
          },
        },
      })
    )

    await useAdminStore.getState().loadCliImages()

    const { cliImages, cliImagesError } = useAdminStore.getState()
    expect(cliImagesError).toBeNull()
    expect(cliImages?.autoUpdateEnabled).toBe(false)
    expect(cliImages?.tools).toHaveLength(1)
    expect(cliImages?.tools[0]?.tool).toBe('codex')
  })

  test('stores a permission step when CLI image status is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'admin only' }))

    await useAdminStore.getState().loadCliImages()

    expectBeginnerError(
      useAdminStore.getState().cliImagesError,
      'You do not have permission to view admin agent work-tool images. Ask an owner to update your admin role.'
    )
  })

  test('rollCliImage stores the per-agent report and refreshes status on success', async () => {
    // First call: the roll POST; second call: the loadCliImages refresh.
    authFetchMock
      .mockResolvedValueOnce(
        response(200, {
          ok: true,
          data: {
            tool: 'codex',
            total: 2,
            succeeded: 2,
            failed: 0,
            skippedBusy: 0,
            results: [
              { agentId: 'a1', ok: true, stopped: false },
              { agentId: 'a2', ok: true, stopped: false },
            ],
          },
        })
      )
      .mockResolvedValueOnce(
        response(200, {
          ok: true,
          data: {
            autoUpdateEnabled: true,
            pollIntervalSecs: 900,
            registry: 'ghcr.io/x',
            imageTag: 'latest',
            tools: [],
            prune: {
              enabled: false,
              lastRunUnix: null,
              scanned: 0,
              removed: 0,
              skippedInUse: 0,
              skippedConflict: 0,
              errors: 0,
              lastError: null,
            },
          },
        })
      )

    await useAdminStore.getState().rollCliImage('codex')

    const state = useAdminStore.getState()
    expect(state.cliImageRollError).toBeNull()
    expect(state.cliImageRollingTool).toBeNull()
    expect(state.cliImageRollResult?.tool).toBe('codex')
    expect(state.cliImageRollResult?.succeeded).toBe(2)
    // refreshed the status report afterward (second fetch consumed).
    expect(authFetchMock).toHaveBeenCalledTimes(2)
  })

  test('rollCliImage surfaces a 409 already-in-progress conflict', async () => {
    authFetchMock.mockResolvedValue(
      response(409, { error: "a roll for 'codex' is already in progress" })
    )

    await useAdminStore.getState().rollCliImage('codex')

    const state = useAdminStore.getState()
    expect(state.cliImageRollingTool).toBeNull()
    expect(state.cliImageRollError).toContain('already in progress')
    expect(state.cliImageRollResult).toBeNull()
  })

  test('still loads admin users on success', async () => {
    authFetchMock.mockResolvedValue(
      response(200, {
        users: [
          {
            id: 'user-1',
            email: 'owner@example.com',
            displayName: 'Owner',
            role: 'owner',
            status: 'active',
            createdAt: null,
            lastLoginAt: null,
            sessionsCount: 1,
          },
        ],
        total: 1,
        page: 1,
      })
    )

    await useAdminStore.getState().loadUsers()

    expect(useAdminStore.getState().users).toHaveLength(1)
    expect(useAdminStore.getState().usersError).toBeNull()
  })
})
