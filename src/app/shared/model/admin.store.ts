import { create } from 'zustand'
import { getAuthFetch } from '@app/shared/api/legacy'

// ============================================================================
// Types
// ============================================================================

export type AdminSection = 'users' | 'organizations' | 'agents' | 'health' | 'cli-images'

/**
 * Canonical runtime-kind discriminator. Mirrors `AgentRuntimeKind` from
 * `@app/entities/agent`, declared locally because the admin store lives in the
 * `shared` FSD layer and must not import upward from `entities`. Feature code
 * (e.g. `AgentsPanel`) still imports the canonical specs/labels from the agent
 * entity.
 */
export type AdminAgentRuntimeKind = 'container' | 'cli' | 'api'

/** Runtime-kind filter value for the admin agents view. `'all'` = no filter. */
export type AdminAgentRuntimeKindFilter = 'all' | AdminAgentRuntimeKind

/** One agent row as returned by `GET /api/v1/admin/agents`. */
export interface AdminAgent {
  id: string
  name: string
  status: string
  runtimeKind: AdminAgentRuntimeKind
  cliTool: string | null
  ownerUsername: string | null
  ownerEmail: string | null
  projectName: string | null
  lastActivity: number
}

/** One user row as returned by `GET /api/v1/admin/users`. */
export interface AdminUser {
  id: string
  email: string
  displayName: string
  /** `'admin' | 'member'` — derived from `users.is_admin` on the backend. */
  role: string
  status: 'active' | 'inactive'
  /** RFC3339 timestamp string. */
  createdAt: string | null
  /** RFC3339 timestamp string, or null when the user never signed in. */
  lastLoginAt: string | null
}

/** One organization row as returned by `GET /api/v1/admin/organizations`. */
export interface AdminOrg {
  id: string
  name: string
  slug: string
  membersCount: number
  teamsCount: number
  createdAt: string
}

export interface ComponentHealth {
  status: 'up' | 'down' | 'degraded'
  latencyMs?: number
  error?: string
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'unhealthy'
  checks: {
    database?: ComponentHealth
    redis?: ComponentHealth
    nats?: ComponentHealth
    docker?: ComponentHealth
  }
  uptime?: number
  version?: string
}

/**
 * One Container CLI tool's image-update state, as returned per-tool by
 * `GET /api/v1/admin/cli-images`. `pending` means the auto-updater has not yet
 * run a check for this tool (it is off by default). `update_available` is
 * claude-only: a newer npm version exists and can be built locally.
 */
export type CliImageToolState = 'pending' | 'up_to_date' | 'update_available' | 'updated' | 'failed'

/**
 * How a tool's image is kept current: pulled from a public registry, or built
 * locally on the server (claude — its license forbids a public image).
 */
export type CliImageUpdateMode = 'registry' | 'local_build'

export interface CliImageTool {
  tool: string
  state: CliImageToolState
  updateMode: CliImageUpdateMode
  localDigest: string | null
  remoteDigest: string | null
  /** Local-build tools: version baked into the local image (null = unknown). */
  localVersion: string | null
  /** Local-build tools: latest version published on npm. */
  remoteVersion: string | null
  /** True while a server-side local build is running for this tool. */
  building: boolean
  lastCheckedUnix: number | null
  lastUpdatedUnix: number | null
  lastError: string | null
  /**
   * Agents that currently have an associated container for this tool. A rough
   * blast-radius hint; it does NOT assert which image digest each live
   * container booted from.
   */
  agentsWithContainer: number
}

/** Most recent superseded-image prune sweep (default-off). */
export interface CliImagePruneStatus {
  enabled: boolean
  lastRunUnix: number | null
  scanned: number
  removed: number
  skippedInUse: number
  skippedConflict: number
  errors: number
  lastError: string | null
}

/** Per-agent outcome of a roll. */
export interface RollAgentResult {
  agentId: string
  ok: boolean
  /**
   * Only meaningful when `ok` is false: `true` = container confirmed
   * stopped+removed but the respawn failed (agent is DOWN — restart it);
   * `false` = the stop did not complete cleanly, so the post-condition is
   * UNCONFIRMED (may still be running on the previous image, or already down
   * from a partial stop) — check the Agents view.
   */
  stopped: boolean
  error?: string
}

/** Result of `POST /api/v1/admin/cli-images/{tool}/roll`. */
export interface CliImageRollReport {
  tool: string
  total: number
  succeeded: number
  failed: number
  /** Working agents intentionally left alone (rolling them would interrupt work). */
  skippedBusy: number
  results: RollAgentResult[]
}

/** Full report from `GET /api/v1/admin/cli-images`. */
export interface CliImageStatus {
  autoUpdateEnabled: boolean
  /** Whether the sweep auto-builds the claude image (zero clicks). */
  claudeAutoBuildEnabled: boolean
  pollIntervalSecs: number
  registry: string
  imageTag: string
  tools: CliImageTool[]
  prune: CliImagePruneStatus
}

interface AdminState {
  // Navigation
  activeSection: AdminSection

  // Users
  users: AdminUser[]
  usersTotal: number
  usersPage: number
  usersLoading: boolean
  usersError: string | null
  userSearch: string
  /**
   * Error from the most recent per-user action (role change or removal).
   * Backend guard rejections (own account, last admin) land here verbatim so
   * the panel can show exactly why the change was refused.
   */
  userActionError: string | null

  // Orgs
  orgs: AdminOrg[]
  orgsLoading: boolean
  orgsError: string | null

  // Agents
  agents: AdminAgent[]
  agentsTotal: number
  agentsLoading: boolean
  agentsError: string | null
  agentRuntimeKindFilter: AdminAgentRuntimeKindFilter

  // Health
  health: SystemHealth | null
  healthLoading: boolean
  healthError: string | null

  // CLI agent images
  cliImages: CliImageStatus | null
  cliImagesLoading: boolean
  cliImagesError: string | null

  // CLI image roll (destructive; operator-initiated)
  cliImageRollingTool: string | null
  cliImageRollResult: CliImageRollReport | null
  cliImageRollError: string | null

  // claude local image build (image-level; never touches running agents)
  cliImageBuildError: string | null

  // Actions
  setActiveSection: (section: AdminSection) => void
  setUserSearch: (search: string) => void

  loadUsers: (page?: number) => Promise<void>
  /**
   * Change a user's access level via `PUT /api/v1/admin/users/{id}`. On
   * success the row is swapped for the backend's updated projection and the
   * call resolves `true`; on rejection the reason lands in `userActionError`
   * and the call resolves `false`.
   */
  updateUserRole: (id: string, role: 'admin' | 'member') => Promise<boolean>
  /**
   * Remove a user account via `DELETE /api/v1/admin/users/{id}` (the backend
   * soft-deletes; sign-in stops immediately). On success the row leaves the
   * list and the total drops by one; on rejection the reason lands in
   * `userActionError`.
   */
  deleteUser: (id: string) => Promise<boolean>
  /** Dismiss the last user-action error (e.g. when the operator cancels). */
  clearUserActionError: () => void

  loadOrgs: () => Promise<void>

  loadAgents: () => Promise<void>
  setAgentRuntimeKindFilter: (filter: AdminAgentRuntimeKindFilter) => Promise<void>

  loadHealth: () => Promise<void>

  loadCliImages: () => Promise<void>
  /**
   * Live-patch one tool from a `cli_image.updated` WebSocket toast so an open
   * panel reflects the change immediately instead of waiting for the 30s poll.
   * No-op when the report has not been loaded yet (the next poll fills it in).
   */
  applyCliImageUpdate: (update: {
    tool: string
    state: 'updated' | 'failed' | 'update_available'
    localDigest: string | null
    remoteDigest: string | null
    localVersion?: string | null
    remoteVersion?: string | null
    lastError: string | null
    unix: number
  }) => void
  /**
   * Roll the running container agents of one tool onto the new image
   * (destructive — interrupts running agents). Sets the in-flight tool, then
   * the per-agent report or an error; refreshes the status report afterward.
   */
  rollCliImage: (tool: string) => Promise<void>
  /**
   * Start a server-side build of the claude agent image (claude has no public
   * registry image). Optimistically marks the claude row as building — the 30s
   * poll (or the completion toast) corrects it. Resolves `true` when the build
   * was accepted (202), `false` on any error (which lands in
   * `cliImageBuildError`). Image-level only; running agents are untouched.
   */
  buildClaudeImage: () => Promise<boolean>
}

type AdminResource = 'users' | 'organizations' | 'agents' | 'health' | 'cli-images'

class AdminUserFacingError extends Error {}

function userFacingError(message: string): AdminUserFacingError {
  return new AdminUserFacingError(message)
}

// ============================================================================
// Helpers
// ============================================================================

/**
 * Admin fetch using the legacy API's authFetch for consistent 401 refresh.
 */
async function adminFetch(path: string, init?: RequestInit): Promise<Response> {
  const authFetch = getAuthFetch()
  return authFetch(path, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers as Record<string, string> | undefined),
    },
  })
}

function adminResourceLabel(resource: AdminResource): string {
  switch (resource) {
    case 'users':
      return 'user list'
    case 'organizations':
      return 'organization list'
    case 'agents':
      return 'agent list'
    case 'health':
      return 'system health'
    case 'cli-images':
      return 'CLI agent images'
  }
}

function adminErrorDetail(data: Record<string, unknown>): string | null {
  if (typeof data.error === 'string' && data.error.trim()) return data.error.trim()
  if (
    data.error &&
    typeof data.error === 'object' &&
    'message' in data.error &&
    typeof data.error.message === 'string' &&
    data.error.message.trim()
  ) {
    return data.error.message.trim()
  }
  if (typeof data.message === 'string' && data.message.trim()) return data.message.trim()
  return null
}

async function readAdminErrorPayload(res: Response): Promise<Record<string, unknown>> {
  return ((await res.json().catch(() => ({}))) ?? {}) as Record<string, unknown>
}

export function adminHttpErrorMessage(
  resource: AdminResource,
  status: number,
  data: Record<string, unknown> = {}
): string {
  const label = adminResourceLabel(resource)
  const detail = adminErrorDetail(data)
  const suffix = detail ? ` Details: ${detail}` : ''
  const statusText = `Code: ${status}.`

  if (status === 401) {
    return `Sign in again, then reload the ${label}. ${statusText}${suffix}`
  }
  if (status === 403) {
    return `You do not have permission to view admin ${label}. Ask an owner to update your admin role. ${statusText}${suffix}`
  }
  if (status === 404) {
    return `The admin ${label} endpoint is not available. Refresh after the backend is deployed. ${statusText}${suffix}`
  }
  if (status === 429) {
    return `The admin service is busy. Wait a moment, then reload the ${label}. ${statusText}${suffix}`
  }
  if (status >= 500) {
    return `The admin service had a server problem. Try again after the backend is healthy. ${statusText}${suffix}`
  }

  return `The admin ${label} could not load. Refresh the page and try again. ${statusText}${suffix}`
}

function adminNetworkErrorMessage(resource: AdminResource): string {
  return `The admin ${adminResourceLabel(resource)} could not load because the browser could not reach the server. Check your connection and refresh the page.`
}

function adminErrorMessage(err: unknown, resource: AdminResource): string {
  return err instanceof AdminUserFacingError ? err.message : adminNetworkErrorMessage(resource)
}

// ---------------------------------------------------------------------------
// Per-user action errors (role change / removal)
// ---------------------------------------------------------------------------

type AdminUserAction = 'change-role' | 'remove'

function adminUserActionLabel(action: AdminUserAction): string {
  return action === 'change-role' ? 'access change' : 'removal'
}

/**
 * Beginner-first message for a failed user action. Guard rejections (own
 * account, last admin, unknown access level) arrive as HTTP 422 with a
 * ready-to-read sentence — those are shown directly, minus the protocol
 * prefix, because the backend wording explains exactly what to do next.
 */
export function adminUserActionErrorMessage(
  action: AdminUserAction,
  status: number,
  data: Record<string, unknown> = {}
): string {
  const label = adminUserActionLabel(action)
  const detail = adminErrorDetail(data)

  if (status === 422 && detail) {
    const message = detail.replace(/^unprocessable entity:\s*/i, '')
    return message.charAt(0).toUpperCase() + message.slice(1)
  }

  const suffix = detail ? ` Details: ${detail}` : ''
  const statusText = `Code: ${status}.`
  if (status === 401) {
    return `Sign in again, then retry the ${label}. ${statusText}${suffix}`
  }
  if (status === 403) {
    return `You do not have permission to manage users. Ask an owner to update your admin role. ${statusText}${suffix}`
  }
  if (status === 404) {
    return `This user is no longer in the list. Reload the user list to see the latest accounts. ${statusText}${suffix}`
  }
  if (status >= 500) {
    return `The admin service had a server problem. Retry the ${label} after the backend is healthy. ${statusText}${suffix}`
  }
  return `The ${label} did not go through. Try again. ${statusText}${suffix}`
}

function adminUserActionNetworkMessage(action: AdminUserAction): string {
  return `The ${adminUserActionLabel(action)} could not reach the server. Check your connection and try again.`
}

function adminUserActionError(err: unknown, action: AdminUserAction): string {
  return err instanceof AdminUserFacingError ? err.message : adminUserActionNetworkMessage(action)
}

// ============================================================================
// Store
// ============================================================================

export const useAdminStore = create<AdminState>((set, get) => ({
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

  agents: [],
  agentsTotal: 0,
  agentsLoading: false,
  agentsError: null,
  agentRuntimeKindFilter: 'all',

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

  setActiveSection: (activeSection) => set({ activeSection }),
  setUserSearch: (userSearch) => set({ userSearch }),

  // ---------------------------------------------------------------------------
  // Users
  // ---------------------------------------------------------------------------

  loadUsers: async (page = 1) => {
    const { userSearch } = get()
    set({ usersLoading: true, usersError: null })
    try {
      const params = new URLSearchParams({ page: String(page), limit: '25' })
      if (userSearch) params.set('search', userSearch)
      const res = await adminFetch(`/api/v1/admin/users?${params.toString()}`)
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('users', res.status, await readAdminErrorPayload(res))
        )
      }
      // Parse `{ users, total, page, totalPages }` defensively: a missing
      // `users` array (e.g. a legacy `{ ok, data }` body) must render an empty
      // table, never crash the page.
      const data = (await res.json().catch(() => ({}))) as {
        users?: AdminUser[]
        total?: number
        page?: number
        totalPages?: number
      } | null
      set({
        users: data?.users ?? [],
        usersTotal: data?.total ?? 0,
        usersPage: data?.page ?? 1,
        usersLoading: false,
      })
    } catch (err) {
      set({ usersLoading: false, usersError: adminErrorMessage(err, 'users') })
    }
  },

  updateUserRole: async (id, role) => {
    set({ userActionError: null })
    try {
      const res = await adminFetch(`/api/v1/admin/users/${encodeURIComponent(id)}`, {
        method: 'PUT',
        body: JSON.stringify({ role }),
      })
      const body = (await res.json().catch(() => null)) as {
        ok?: boolean
        user?: AdminUser
      } | null
      if (!res.ok || !body || body.ok === false || !body.user) {
        throw userFacingError(
          adminUserActionErrorMessage(
            'change-role',
            res.status,
            (body ?? {}) as Record<string, unknown>
          )
        )
      }
      // Swap in the backend's updated projection so the row shows exactly
      // what was saved (role, status, timestamps) — no optimistic guessing.
      const updated = body.user
      set((s) => ({ users: s.users.map((user) => (user.id === id ? updated : user)) }))
      return true
    } catch (err) {
      set({ userActionError: adminUserActionError(err, 'change-role') })
      return false
    }
  },

  deleteUser: async (id) => {
    set({ userActionError: null })
    try {
      const res = await adminFetch(`/api/v1/admin/users/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      })
      const body = (await res.json().catch(() => null)) as { ok?: boolean } | null
      if (!res.ok || !body || body.ok === false) {
        throw userFacingError(
          adminUserActionErrorMessage('remove', res.status, (body ?? {}) as Record<string, unknown>)
        )
      }
      set((s) => ({
        users: s.users.filter((user) => user.id !== id),
        usersTotal: Math.max(0, s.usersTotal - 1),
      }))
      return true
    } catch (err) {
      set({ userActionError: adminUserActionError(err, 'remove') })
      return false
    }
  },

  clearUserActionError: () => set({ userActionError: null }),

  // ---------------------------------------------------------------------------
  // Orgs
  // ---------------------------------------------------------------------------

  loadOrgs: async () => {
    set({ orgsLoading: true, orgsError: null })
    try {
      const res = await adminFetch('/api/v1/admin/organizations')
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('organizations', res.status, await readAdminErrorPayload(res))
        )
      }
      const data = (await res.json().catch(() => ({}))) as {
        organizations?: AdminOrg[]
        total?: number
      } | null
      set({ orgs: data?.organizations ?? [], orgsLoading: false })
    } catch (err) {
      set({ orgsLoading: false, orgsError: adminErrorMessage(err, 'organizations') })
    }
  },

  // ---------------------------------------------------------------------------
  // Agents
  // ---------------------------------------------------------------------------

  loadAgents: async () => {
    const { agentRuntimeKindFilter } = get()
    set({ agentsLoading: true, agentsError: null })
    try {
      const params = new URLSearchParams({ page: '1', limit: '100' })
      if (agentRuntimeKindFilter !== 'all') {
        params.set('runtimeKind', agentRuntimeKindFilter)
      }
      const res = await adminFetch(`/api/v1/admin/agents?${params.toString()}`)
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('agents', res.status, await readAdminErrorPayload(res))
        )
      }
      const data = (await res.json()) as { agents: AdminAgent[]; total: number }
      set({ agents: data.agents, agentsTotal: data.total, agentsLoading: false })
    } catch (err) {
      set({ agentsLoading: false, agentsError: adminErrorMessage(err, 'agents') })
    }
  },

  setAgentRuntimeKindFilter: async (filter) => {
    set({ agentRuntimeKindFilter: filter })
    await get().loadAgents()
  },

  // ---------------------------------------------------------------------------
  // Health
  // ---------------------------------------------------------------------------

  loadHealth: async () => {
    set({ healthLoading: true, healthError: null })
    try {
      // `GET /api/health` is the deep readiness probe. It answers
      // `{ ok, status: 'ready' | 'degraded', checks: { database, redis, nats,
      // docker } }` with boolean checks — and uses HTTP 503 when a required
      // dependency is down, so a body that still carries `checks` is a health
      // REPORT to render, not a failed request.
      const res = await adminFetch('/api/health')
      const body = (await res.json().catch(() => null)) as {
        ok?: boolean
        status?: string
        checks?: {
          database?: boolean
          redis?: boolean
          nats?: boolean
          docker?: boolean
        }
      } | null
      if (!body || typeof body.checks !== 'object' || body.checks === null) {
        throw userFacingError(
          adminHttpErrorMessage('health', res.status, (body ?? {}) as Record<string, unknown>)
        )
      }
      const toComponent = (up: boolean | undefined): ComponentHealth => ({
        status: up ? 'up' : 'down',
      })
      const health: SystemHealth = {
        status: body.ok === false ? 'unhealthy' : body.status === 'ready' ? 'healthy' : 'degraded',
        checks: {
          database: toComponent(body.checks.database),
          redis: toComponent(body.checks.redis),
          nats: toComponent(body.checks.nats),
          docker: toComponent(body.checks.docker),
        },
      }
      set({ health, healthLoading: false })
    } catch (err) {
      set({ healthLoading: false, healthError: adminErrorMessage(err, 'health') })
    }
  },

  // ---------------------------------------------------------------------------
  // CLI agent images
  // ---------------------------------------------------------------------------

  loadCliImages: async () => {
    set({ cliImagesLoading: true, cliImagesError: null })
    try {
      const res = await adminFetch('/api/v1/admin/cli-images')
      if (!res.ok) {
        throw userFacingError(
          adminHttpErrorMessage('cli-images', res.status, await readAdminErrorPayload(res))
        )
      }
      // Validate the `{ ok, data }` envelope before trusting it. A 200 with a
      // missing/false `ok`, absent `data`, or non-array `tools` must surface as
      // an error — not render a blank panel that looks like success.
      const body = (await res.json().catch(() => null)) as {
        ok?: boolean
        data?: CliImageStatus
      } | null
      if (!body || body.ok === false || !body.data || !Array.isArray(body.data.tools)) {
        throw userFacingError(
          adminHttpErrorMessage('cli-images', res.status, (body ?? {}) as Record<string, unknown>)
        )
      }
      set({ cliImages: body.data, cliImagesLoading: false })
    } catch (err) {
      set({ cliImagesLoading: false, cliImagesError: adminErrorMessage(err, 'cli-images') })
    }
  },

  applyCliImageUpdate: (update) =>
    set((s) => {
      if (!s.cliImages) return {}
      const tools = s.cliImages.tools.map((t) =>
        t.tool === update.tool
          ? {
              ...t,
              state: update.state,
              localDigest: update.localDigest,
              remoteDigest: update.remoteDigest,
              localVersion: update.localVersion ?? t.localVersion,
              remoteVersion: update.remoteVersion ?? t.remoteVersion,
              // `updated`/`failed` are build/check outcomes — the build (if
              // any) is over. `update_available` leaves an in-flight flag as-is.
              building: update.state === 'update_available' ? t.building : false,
              lastError: update.lastError,
              lastCheckedUnix: update.unix,
              lastUpdatedUnix: update.state === 'updated' ? update.unix : t.lastUpdatedUnix,
            }
          : t
      )
      return { cliImages: { ...s.cliImages, tools } }
    }),

  rollCliImage: async (tool) => {
    // Starting a new roll clears the prior result so a stale report can't read
    // as the outcome of this attempt.
    set({ cliImageRollingTool: tool, cliImageRollError: null, cliImageRollResult: null })
    try {
      const res = await adminFetch(`/api/v1/admin/cli-images/${encodeURIComponent(tool)}/roll`, {
        method: 'POST',
      })
      const body = (await res.json().catch(() => null)) as {
        ok?: boolean
        data?: CliImageRollReport
      } | null
      if (!res.ok || !body || body.ok === false || !body.data) {
        throw userFacingError(
          adminHttpErrorMessage('cli-images', res.status, (body ?? {}) as Record<string, unknown>)
        )
      }
      set({ cliImageRollingTool: null, cliImageRollResult: body.data })
      // Roll changed which agents have containers — refresh the status report.
      await get().loadCliImages()
    } catch (err) {
      set({ cliImageRollingTool: null, cliImageRollError: adminErrorMessage(err, 'cli-images') })
    }
  },

  buildClaudeImage: async () => {
    const patchClaudeBuilding = (building: boolean) =>
      set((s) => {
        if (!s.cliImages) return {}
        const tools = s.cliImages.tools.map((t) => (t.tool === 'claude' ? { ...t, building } : t))
        return { cliImages: { ...s.cliImages, tools } }
      })

    set({ cliImageBuildError: null })
    // Optimistic: show "building" immediately; the 30s poll refresh (or the
    // completion toast) carries the server's real state afterward.
    patchClaudeBuilding(true)
    try {
      const res = await adminFetch('/api/v1/admin/cli-images/claude/build', { method: 'POST' })
      const body = (await res.json().catch(() => null)) as {
        ok?: boolean
        started?: boolean
        targetVersion?: string
      } | null
      if (!res.ok || !body || body.ok === false) {
        throw userFacingError(
          adminHttpErrorMessage('cli-images', res.status, (body ?? {}) as Record<string, unknown>)
        )
      }
      return true
    } catch (err) {
      // The build did not start — roll the optimistic flag back so the Build
      // button unlocks, and explain what happened.
      patchClaudeBuilding(false)
      set({ cliImageBuildError: adminErrorMessage(err, 'cli-images') })
      return false
    }
  },
}))
