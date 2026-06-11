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
    cliImageBuildError: null,
  })
}

beforeEach(() => {
  resetAdminState()
  authFetchMock.mockReset()
})

describe('adminHttpErrorMessage', () => {
  test('turns expired admin auth into a sign-in step', () => {
    expect(adminHttpErrorMessage('users', 401)).toBe(
      'Sign in again, then reload the user list. Code: 401.'
    )
  })

  test('turns admin permission failures into an owner role step', () => {
    expect(adminHttpErrorMessage('organizations', 403)).toBe(
      'You do not have permission to view admin organization list. Ask an owner to update your admin role. Code: 403.'
    )
  })

  test('keeps backend detail after the operator action', () => {
    expect(adminHttpErrorMessage('health', 503, { error: { message: 'database down' } })).toBe(
      'The admin service had a server problem. Try again after the backend is healthy. Code: 503. Details: database down'
    )
  })
})

describe('useAdminStore loading errors', () => {
  test('stores beginner guidance when user loading is forbidden', async () => {
    authFetchMock.mockResolvedValue(response(403, { error: 'owner role required' }))

    await useAdminStore.getState().loadUsers()

    expect(useAdminStore.getState().usersError).toBe(
      'You do not have permission to view admin user list. Ask an owner to update your admin role. Code: 403. Details: owner role required'
    )
  })

  test('stores a connection recovery step when organization loading cannot reach the server', async () => {
    authFetchMock.mockRejectedValue(new TypeError('Failed to fetch'))

    await useAdminStore.getState().loadOrgs()

    expect(useAdminStore.getState().orgsError).toBe(
      'The admin organization list could not load because the browser could not reach the server. Check your connection and refresh the page.'
    )
  })

  test('stores service recovery guidance when health loading fails', async () => {
    authFetchMock.mockResolvedValue(response(503, { message: 'health database unavailable' }))

    await useAdminStore.getState().loadHealth()

    expect(useAdminStore.getState().healthError).toBe(
      'The admin service had a server problem. Try again after the backend is healthy. Code: 503. Details: health database unavailable'
    )
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

    expect(useAdminStore.getState().cliImagesError).toBe(
      'You do not have permission to view admin CLI agent images. Ask an owner to update your admin role. Code: 403. Details: admin only'
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
