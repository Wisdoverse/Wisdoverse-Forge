import { beforeEach, describe, expect, test, vi } from 'vitest'

const authFetchMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/legacy', () => ({
  getAuthFetch: () => authFetchMock,
}))

import {
  adminHttpErrorMessage,
  adminUserActionErrorMessage,
  useAdminStore,
  type AdminUser,
} from '@app/shared/model/admin.store'

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
    userActionError: null,
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
    cliImageBuildError: null,
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
      'Your sign-in expired. Sign in again, then open Admin and reload the user list.'
    )
  })

  test('turns admin permission failures into an Admin access step', () => {
    expectBeginnerError(
      adminHttpErrorMessage('organizations', 403),
      'You do not have access to the admin team space list. Ask an owner or admin to give you Admin access, then reload Admin.'
    )
  })

  test('turns server failures into an owner recovery step', () => {
    const message = adminHttpErrorMessage('health', 503, {
      error: { message: 'database down' },
    })

    expectBeginnerError(
      message,
      'Reload the system health, then try again. Forge could not load the admin system health right now. If it still fails, ask an owner or admin to check Admin setup.'
    )
    expect(message).not.toContain('temporarily unavailable')
    expect(message).not.toContain('admin service')
  })

  test('turns missing admin resources into an Admin setup step', () => {
    const message = adminHttpErrorMessage('agents', 404)

    expectBeginnerError(
      message,
      'Refresh Admin, then try again. The admin agent list is not available from this Admin view. If it still fails, ask an owner or admin to check setup.'
    )
    expect(message).not.toContain('service')
  })

  test('turns admin rate limits into a wait and reload step', () => {
    expectBeginnerError(
      adminHttpErrorMessage('users', 429),
      'Forge is receiving too many Admin requests right now. Wait a moment, then reload the user list.'
    )
  })
})

describe('useAdminStore loading errors', () => {
  test('stores beginner guidance when user loading is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'owner role required' }))

    await useAdminStore.getState().loadUsers()

    expectBeginnerError(
      useAdminStore.getState().usersError,
      'You do not have access to the admin user list. Ask an owner or admin to give you Admin access, then reload Admin.'
    )
    expect(useAdminStore.getState().usersError).not.toContain('role')
  })

  test('stores a connection recovery step when organization loading cannot reach the server', async () => {
    authFetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await useAdminStore.getState().loadOrgs()

    expect(useAdminStore.getState().orgsError).toBe(
      'Check your connection, then refresh Admin. Forge could not connect while loading the admin team space list.'
    )
    expect(useAdminStore.getState().orgsError).not.toContain('could not reach the service')
  })

  test('returns beginner guidance when user access saving is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'owner role required' }))

    const result = await useAdminStore.getState().updateUserRole('user-1', 'member')

    expect(result).toBe(false)
    expectBeginnerError(
      useAdminStore.getState().userActionError,
      'You do not have access to change user access. Ask an owner or admin to give you Admin access, then save again.'
    )
    expect(useAdminStore.getState().userActionError).not.toContain('role')
  })

  test('stores service recovery guidance when health loading fails', async () => {
    authFetchMock.mockResolvedValue(response(503, { message: 'health database unavailable' }))

    await useAdminStore.getState().loadHealth()

    expectBeginnerError(
      useAdminStore.getState().healthError,
      'Reload the system health, then try again. Forge could not load the admin system health right now. If it still fails, ask an owner or admin to check Admin setup.'
    )
    expect(useAdminStore.getState().healthError).not.toContain('temporarily unavailable')
  })

  test('loads the CLI image status report on success', async () => {
    authFetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        data: {
          autoUpdateEnabled: false,
          claudeAutoBuildEnabled: false,
          pollIntervalSecs: 900,
          registry: 'ghcr.io/wisdoverse/wisdoverse-forge',
          imageTag: 'latest',
          tools: [
            {
              tool: 'claude',
              state: 'update_available',
              updateMode: 'local_build',
              localDigest: null,
              remoteDigest: null,
              localVersion: '2.1.100',
              remoteVersion: '2.1.173',
              building: false,
              lastCheckedUnix: null,
              lastUpdatedUnix: null,
              lastError: null,
              agentsWithContainer: 0,
            },
            {
              tool: 'codex',
              state: 'pending',
              updateMode: 'registry',
              localDigest: null,
              remoteDigest: null,
              localVersion: null,
              remoteVersion: null,
              building: false,
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
    expect(cliImages?.claudeAutoBuildEnabled).toBe(false)
    expect(cliImages?.tools).toHaveLength(2)
    // claude is a first-class row with the local-build contract fields.
    expect(cliImages?.tools[0]?.tool).toBe('claude')
    expect(cliImages?.tools[0]?.updateMode).toBe('local_build')
    expect(cliImages?.tools[0]?.localVersion).toBe('2.1.100')
    expect(cliImages?.tools[0]?.remoteVersion).toBe('2.1.173')
    expect(cliImages?.tools[0]?.building).toBe(false)
    expect(cliImages?.tools[1]?.tool).toBe('codex')
  })

  test('stores a permission step when CLI image status is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'admin only' }))

    await useAdminStore.getState().loadCliImages()

    expectBeginnerError(
      useAdminStore.getState().cliImagesError,
      'You do not have access to the admin agent tool updates. Ask an owner or admin to give you Admin access, then reload Admin.'
    )
    expect(useAdminStore.getState().cliImagesError).not.toContain('role')
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
            claudeAutoBuildEnabled: false,
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

  /** A loaded report with a claude row, for build-action tests. */
  function seedClaudeReport(overrides: { building?: boolean; state?: string } = {}) {
    useAdminStore.setState({
      cliImages: {
        autoUpdateEnabled: true,
        claudeAutoBuildEnabled: false,
        pollIntervalSecs: 900,
        registry: 'ghcr.io/x',
        imageTag: 'latest',
        tools: [
          {
            tool: 'claude',
            state: (overrides.state ?? 'update_available') as never,
            updateMode: 'local_build',
            localDigest: null,
            remoteDigest: null,
            localVersion: '2.1.100',
            remoteVersion: '2.1.173',
            building: overrides.building ?? false,
            lastCheckedUnix: 1_700_000_000,
            lastUpdatedUnix: null,
            lastError: null,
            agentsWithContainer: 1,
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
  }

  test('buildClaudeImage posts to the build endpoint and marks claude building', async () => {
    seedClaudeReport()
    authFetchMock.mockResolvedValue(
      response(202, { ok: true, started: true, targetVersion: '2.1.173' })
    )

    const started = await useAdminStore.getState().buildClaudeImage()

    expect(started).toBe(true)
    expect(authFetchMock).toHaveBeenCalledWith(
      '/api/v1/admin/cli-images/claude/build',
      expect.objectContaining({ method: 'POST' })
    )
    const state = useAdminStore.getState()
    expect(state.cliImageBuildError).toBeNull()
    // optimistic flag stays set; the next poll/toast carries the real state.
    expect(state.cliImages?.tools[0]?.building).toBe(true)
  })

  test('buildClaudeImage rolls back the building flag on a 409 conflict', async () => {
    seedClaudeReport()
    authFetchMock.mockResolvedValue(
      response(409, { error: 'a claude image build is already in progress' })
    )

    const started = await useAdminStore.getState().buildClaudeImage()

    expect(started).toBe(false)
    const state = useAdminStore.getState()
    expect(state.cliImageBuildError).toContain('already in progress')
    // the build did not start — the button must unlock again.
    expect(state.cliImages?.tools[0]?.building).toBe(false)
  })

  test('still loads admin users on success', async () => {
    authFetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        users: [
          {
            id: 'user-1',
            email: 'owner@example.com',
            displayName: 'Owner',
            role: 'admin',
            status: 'active',
            createdAt: '2026-05-01T12:00:00Z',
            lastLoginAt: null,
          },
        ],
        total: 1,
        page: 1,
        totalPages: 1,
      })
    )

    await useAdminStore.getState().loadUsers()

    const state = useAdminStore.getState()
    expect(state.users).toHaveLength(1)
    expect(state.users[0]?.displayName).toBe('Owner')
    expect(state.usersTotal).toBe(1)
    expect(state.usersPage).toBe(1)
    expect(state.usersError).toBeNull()
  })

  test('sends page, limit, and search to the admin users endpoint', async () => {
    authFetchMock.mockResolvedValue(
      response(200, { ok: true, users: [], total: 0, page: 2, totalPages: 0 })
    )
    useAdminStore.setState({ userSearch: 'alice' })

    await useAdminStore.getState().loadUsers(2)

    expect(authFetchMock).toHaveBeenCalledWith(
      '/api/v1/admin/users?page=2&limit=25&search=alice',
      expect.anything()
    )
  })

  // ---------------------------------------------------------------------------
  // Per-user actions: role change + removal
  // ---------------------------------------------------------------------------

  const seedUsers = (): AdminUser[] => [
    {
      id: 'user-1',
      email: 'alex@example.com',
      displayName: 'Alex Operator',
      role: 'member',
      status: 'active',
      createdAt: '2026-05-01T12:00:00Z',
      lastLoginAt: null,
    },
    {
      id: 'user-2',
      email: 'bo@example.com',
      displayName: 'Bo Member',
      role: 'member',
      status: 'active',
      createdAt: '2026-05-02T12:00:00Z',
      lastLoginAt: null,
    },
  ]

  test('updateUserRole PUTs the new role and swaps in the saved row', async () => {
    useAdminStore.setState({ users: seedUsers(), usersTotal: 2 })
    authFetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        user: {
          id: 'user-1',
          email: 'alex@example.com',
          displayName: 'Alex Operator',
          role: 'admin',
          status: 'active',
          createdAt: '2026-05-01T12:00:00Z',
          lastLoginAt: null,
        },
      })
    )

    const ok = await useAdminStore.getState().updateUserRole('user-1', 'admin')

    expect(ok).toBe(true)
    expect(authFetchMock).toHaveBeenCalledWith(
      '/api/v1/admin/users/user-1',
      expect.objectContaining({ method: 'PUT', body: JSON.stringify({ role: 'admin' }) })
    )
    const state = useAdminStore.getState()
    expect(state.userActionError).toBeNull()
    expect(state.users[0]?.role).toBe('admin')
    // Other rows are untouched.
    expect(state.users[1]?.role).toBe('member')
    expect(state.usersTotal).toBe(2)
  })

  test('updateUserRole surfaces the backend last-admin guard message verbatim', async () => {
    useAdminStore.setState({ users: seedUsers(), usersTotal: 2 })
    authFetchMock.mockResolvedValue(
      response(422, {
        ok: false,
        error: {
          code: 'UNPROCESSABLE_ENTITY',
          message:
            'unprocessable entity: this is the only admin account left. Make another person an admin first, then retry this change.',
        },
      })
    )

    const ok = await useAdminStore.getState().updateUserRole('user-1', 'member')

    expect(ok).toBe(false)
    expect(useAdminStore.getState().userActionError).toBe(
      'This is the only admin account left. Make another person an admin first, then retry this change.'
    )
    // The row keeps its previous role — nothing was saved.
    expect(useAdminStore.getState().users[0]?.role).toBe('member')
  })

  test('deleteUser DELETEs the account and drops the row and total', async () => {
    useAdminStore.setState({ users: seedUsers(), usersTotal: 2 })
    authFetchMock.mockResolvedValue(response(200, { ok: true }))

    const ok = await useAdminStore.getState().deleteUser('user-2')

    expect(ok).toBe(true)
    expect(authFetchMock).toHaveBeenCalledWith(
      '/api/v1/admin/users/user-2',
      expect.objectContaining({ method: 'DELETE' })
    )
    const state = useAdminStore.getState()
    expect(state.users.map((u) => u.id)).toEqual(['user-1'])
    expect(state.usersTotal).toBe(1)
    expect(state.userActionError).toBeNull()
  })

  test('deleteUser surfaces the backend self-removal guard message verbatim', async () => {
    useAdminStore.setState({ users: seedUsers(), usersTotal: 2 })
    authFetchMock.mockResolvedValue(
      response(422, {
        ok: false,
        error: {
          code: 'UNPROCESSABLE_ENTITY',
          message:
            'unprocessable entity: you cannot change or remove your own account. Ask another admin to make this change for you.',
        },
      })
    )

    const ok = await useAdminStore.getState().deleteUser('user-1')

    expect(ok).toBe(false)
    expect(useAdminStore.getState().userActionError).toBe(
      'You cannot change or remove your own account. Ask another admin to make this change for you.'
    )
    // Nothing was removed.
    expect(useAdminStore.getState().users).toHaveLength(2)
    expect(useAdminStore.getState().usersTotal).toBe(2)
  })

  test('a user action that cannot reach the server explains the retry step', async () => {
    useAdminStore.setState({ users: seedUsers(), usersTotal: 2 })
    authFetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    const ok = await useAdminStore.getState().deleteUser('user-2')

    expect(ok).toBe(false)
    expect(useAdminStore.getState().userActionError).toBe(
      'Check your connection, then try again. The removal could not reach the server.'
    )
    expect(useAdminStore.getState().users).toHaveLength(2)
  })

  test('clearUserActionError dismisses a stale action error', () => {
    useAdminStore.setState({ userActionError: 'old problem' })
    useAdminStore.getState().clearUserActionError()
    expect(useAdminStore.getState().userActionError).toBeNull()
  })

  test('adminUserActionErrorMessage maps statuses to operator steps', () => {
    expect(adminUserActionErrorMessage('change-role', 401)).toBe(
      'Your sign-in expired. Sign in again, then retry the access change.'
    )
    expect(adminUserActionErrorMessage('remove', 403)).toBe(
      'You do not have access to remove user accounts. Ask an owner or admin to give you Admin access, then try again.'
    )
    expect(adminUserActionErrorMessage('remove', 404)).toBe(
      'This user is no longer in the list. Reload the user list to see the latest accounts.'
    )
    expect(adminUserActionErrorMessage('change-role', 500, { error: 'db down' })).toBe(
      'Reload the user list, then try again. Forge could not finish the access change right now. If it still fails, ask an owner or admin to check Admin setup.'
    )
    // 422 without a usable detail falls back to the generic retry step.
    expect(adminUserActionErrorMessage('change-role', 422)).toBe(
      'Refresh the user list, then try again. The access change did not go through.'
    )
    expect(adminUserActionErrorMessage('change-role', 500, { error: 'db down' })).not.toContain(
      'db down'
    )
  })

  test('a legacy {ok,data} users body leaves users empty instead of crashing', async () => {
    // Regression: the old backend answered `{ ok, data }` with raw rows; a body
    // without a `users` array must produce an empty list, never a throw.
    authFetchMock.mockResolvedValue(response(200, { ok: true, data: [{ id: 'user-1' }] }))

    await expect(useAdminStore.getState().loadUsers()).resolves.toBeUndefined()

    const state = useAdminStore.getState()
    expect(state.users).toEqual([])
    expect(state.usersTotal).toBe(0)
    expect(state.usersPage).toBe(1)
    expect(state.usersLoading).toBe(false)
  })

  test('loads organizations from the organizations endpoint contract', async () => {
    authFetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        organizations: [
          {
            id: 'org-1',
            name: 'Acme Labs',
            slug: 'acme',
            membersCount: 6,
            teamsCount: 2,
            createdAt: '2026-05-01T10:00:00Z',
          },
        ],
        total: 1,
      })
    )

    await useAdminStore.getState().loadOrgs()

    expect(authFetchMock).toHaveBeenCalledWith('/api/v1/admin/organizations', expect.anything())
    const state = useAdminStore.getState()
    expect(state.orgs).toHaveLength(1)
    expect(state.orgs[0]?.membersCount).toBe(6)
    expect(state.orgsError).toBeNull()
  })

  test('an organizations body without the array leaves orgs empty', async () => {
    authFetchMock.mockResolvedValue(response(200, { ok: true, data: [] }))

    await useAdminStore.getState().loadOrgs()

    expect(useAdminStore.getState().orgs).toEqual([])
    expect(useAdminStore.getState().orgsError).toBeNull()
  })

  test('maps the readiness probe booleans into service health', async () => {
    authFetchMock.mockResolvedValue(
      response(200, {
        ok: true,
        status: 'ready',
        checks: { database: true, redis: true, nats: true, docker: true },
      })
    )

    await useAdminStore.getState().loadHealth()

    const { health, healthError } = useAdminStore.getState()
    expect(authFetchMock).toHaveBeenCalledWith('/api/health', expect.anything())
    expect(healthError).toBeNull()
    expect(health?.status).toBe('healthy')
    expect(health?.checks.database?.status).toBe('up')
    expect(health?.checks.docker?.status).toBe('up')
  })

  test('a 503 readiness report still renders which checks are down', async () => {
    // /api/health answers HTTP 503 with the SAME body shape when a required
    // dependency is down — that is a report to show, not a request failure.
    authFetchMock.mockResolvedValue(
      response(503, {
        ok: false,
        status: 'degraded',
        checks: { database: false, redis: true, nats: true, docker: true },
      })
    )

    await useAdminStore.getState().loadHealth()

    const { health, healthError } = useAdminStore.getState()
    expect(healthError).toBeNull()
    expect(health?.status).toBe('unhealthy')
    expect(health?.checks.database?.status).toBe('down')
    expect(health?.checks.redis?.status).toBe('up')
  })
})
