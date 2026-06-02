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
