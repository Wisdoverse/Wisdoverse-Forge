/**
 * Context-scoped API mocks for the `react-app-smoke` E2E suite.
 *
 * The important property is that `installStandardMocks` is called against a
 * `BrowserContext`, not a `Page`. Per-context `route` handlers are in place
 * BEFORE any page is created in that context, so they cannot race the first
 * navigation — which was the root cause of the blank-page flake tracked in
 * issue #61 and of the multi-org mock race in issue #63.
 *
 * Per-test overrides (e.g. returning two orgs instead of one) are layered
 * via `overrideOrgs` / `overrideTeams`, which install additional context
 * routes. Playwright evaluates route handlers in reverse order of
 * registration, so the override wins without touching the base mock.
 */

import type { BrowserContext, Route } from '@playwright/test'

// ── Mock data ───────────────────────────────────────────────────────────────

export interface MockOrg {
  id: string
  name: string
  slug: string
  plan: string
  role: string
}

export interface MockTeam {
  id: string
  orgId: string
  name: string
  slug: string
  visibility: string
  description: string
}

export interface MockProject {
  id: string
  teamId: string
  name: string
  slug: string
  color: string
  description: string
}

export const MOCK_ORG: MockOrg = {
  id: 'org-1',
  name: 'Test Org',
  slug: 'test-org',
  plan: 'pro',
  role: 'admin',
}

export const MOCK_ORG_2: MockOrg = {
  id: 'org-2',
  name: 'Acme Corp',
  slug: 'acme-corp',
  plan: 'free',
  role: 'member',
}

export const MOCK_TEAM: MockTeam = {
  id: 'team-1',
  orgId: 'org-1',
  name: 'Engineering',
  slug: 'engineering',
  visibility: 'private',
  description: '',
}

export const MOCK_TEAM_2: MockTeam = {
  id: 'team-2',
  orgId: 'org-1',
  name: 'Design',
  slug: 'design',
  visibility: 'private',
  description: '',
}

export const MOCK_PROJECT: MockProject = {
  id: 'proj-1',
  teamId: 'team-1',
  name: 'Wisdoverse Forge',
  slug: 'agentforge',
  color: '#007AFF',
  description: 'Main project',
}

export const MOCK_PROJECT_2: MockProject = {
  id: 'proj-2',
  teamId: 'team-2',
  name: 'Marketing Site',
  slug: 'marketing-site',
  color: '#FF9500',
  description: 'Marketing project',
}

export const MOCK_GROUP = { id: 'grp-1', name: 'Default', projectId: 'proj-1' } as const

export const MOCK_AGENTS = [
  {
    id: 'agent-container-cli',
    name: 'Codex Container',
    runtimeId: 'af-codex-container',
    containerId: 'container-1234567890ab',
    status: 'idle',
    createdAt: Date.now() - 86_400_000,
    lastActivity: Date.now() - 60_000,
    cwd: '/workspace/agentforge',
    cliTool: 'codex',
    provider: null,
    model: null,
  },
  {
    id: 'agent-provider-prompt',
    name: 'OpenAI Planner',
    runtimeId: 'af-openai-planner',
    status: 'idle',
    createdAt: Date.now() - 43_200_000,
    lastActivity: Date.now() - 120_000,
    provider: 'openai',
    model: 'gpt-5.5',
    systemPrompt: 'Plan concise implementation steps.',
  },
] as const

export function makeMockTask(
  id: string,
  title: string,
  state: string,
  priority = 'normal',
  progress = 0,
  assignedAgentName?: string
) {
  return {
    id,
    groupId: 'grp-1',
    state,
    method: 'agents/run',
    params: { task: title, message: '' },
    assignedTo: assignedAgentName ? `agent-${id}` : undefined,
    assignedAgentName,
    priority,
    progress,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  }
}

export const MOCK_TASKS = [
  makeMockTask('t-001', 'Implement login flow', 'backlog', 'high'),
  makeMockTask('t-002', 'Fix database migration', 'queued', 'urgent'),
  makeMockTask('t-003', 'Write unit tests for auth module', 'working', 'normal', 45, 'Claude'),
  makeMockTask('t-004', 'Review PR #42', 'blocked', 'high', 0, 'GPT-4'),
  makeMockTask('t-005', 'Deploy v2.1.0 to staging', 'completed', 'normal'),
  makeMockTask('t-006', 'Update README', 'backlog', 'low'),
  makeMockTask('t-007', 'Configure CI pipeline', 'working', 'normal', 80, 'Claude'),
]

export interface MockContextFeatureSnapshot {
  governance: boolean
  preview: boolean
  injection: boolean
  analytics: boolean
}

export const MOCK_CONTEXT_FEATURES_ENABLED: MockContextFeatureSnapshot = {
  governance: true,
  preview: true,
  injection: true,
  analytics: true,
}

// ── Context-level mock install ──────────────────────────────────────────────

function json(route: Route, body: unknown): Promise<void> {
  return route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(body),
  })
}

/**
 * Install the default mock set on a browser context. Idempotent per context
 * (calling it twice leaves the newest handler winning, same as any other
 * Playwright route override — intentional, matches the override helpers).
 */
export async function installStandardMocks(context: BrowserContext): Promise<void> {
  // Navigation API — default single org / single team / single project.
  await context.route('**/api/v1/orgs', (route) => json(route, { ok: true, orgs: [MOCK_ORG] }))
  await context.route('**/api/v1/orgs/*/teams', (route) =>
    json(route, { ok: true, teams: [MOCK_TEAM] })
  )
  await context.route('**/api/v1/teams/*/projects', (route) => {
    const projects = route.request().url().includes('team-2') ? [MOCK_PROJECT_2] : [MOCK_PROJECT]
    return json(route, { ok: true, projects })
  })
  await context.route('**/api/v1/groups?*', (route) =>
    json(route, { ok: true, groups: [MOCK_GROUP] })
  )

  await mockContextFeatures(context, MOCK_CONTEXT_FEATURES_ENABLED)

  // Orchestration API — read-only reads + no-op mutations so tests never
  // touch real data even when a click path hits them.
  await context.route('**/api/v1/orchestration/groups/*/tasks/stats', (route) =>
    json(route, {
      ok: true,
      stats: {
        byState: { backlog: 2, queued: 1, working: 2, blocked: 1, completed: 1 },
      },
    })
  )
  await context.route('**/api/v1/orchestration/groups/*/tasks*', (route) =>
    json(route, { ok: true, tasks: MOCK_TASKS })
  )
  await context.route('**/api/v1/orchestration/participants?*', (route) =>
    json(route, {
      ok: true,
      participants: [
        {
          id: 'participant-1',
          agentId: 'agent-preview-1',
          name: 'Codex Preview',
          status: 'available',
          capabilities: ['codex', 'context'],
          lastHeartbeatAt: new Date().toISOString(),
        },
      ],
    })
  )
  await context.route('**/api/v1/orchestration/tasks/*/cancel', (route) =>
    json(route, { ok: true })
  )
  await context.route('**/api/v1/orchestration/tasks/*', (route) => {
    if (route.request().method() === 'PATCH') return json(route, { ok: true })
    return route.continue()
  })
  await context.route('**/api/v1/orchestration/tasks/*/context', (route) =>
    json(route, {
      ok: true,
      data: {
        taskId: 't-003',
        runs: [
          {
            id: 'run-context-1',
            status: 'completed',
            agentId: 'agent-t-003',
            startedAt: new Date(Date.now() - 600_000).toISOString(),
            finishedAt: new Date(Date.now() - 300_000).toISOString(),
            capabilityProfile: { cliTool: 'claude' },
          },
        ],
        appliedItems: [
          {
            injectionId: 'inj-memory-1',
            runId: 'run-context-1',
            itemId: 'memory-prod-ext',
            itemKind: 'memory',
            position: 0,
            title: 'Prod-ext validation memory',
            contentPreview: 'Run make prod-ext and verify API, orchestrator, and NATS health.',
            contentTruncated: false,
            contentRef: 'memory_items/memory-prod-ext',
            scopeKind: 'project',
            scopeId: 'proj-1',
            sensitivity: 'internal',
            state: 'active',
            revoked: false,
            sourceTaskId: 't-001',
            sourceRunId: null,
            source: {
              sourceType: 'memory_item',
              sourceId: 'memory-prod-ext',
              title: 'Prod-ext validation memory',
            },
            lastUsedAt: new Date(Date.now() - 300_000).toISOString(),
            lastVerifiedAt: new Date(Date.now() - 120_000).toISOString(),
            appliedAt: new Date(Date.now() - 300_000).toISOString(),
            adapter: 'claude',
            envelopeVersion: 'v1',
            capabilityProfile: { cliTool: 'claude' },
            degradationReason: null,
            feedback: null,
          },
          {
            injectionId: 'inj-skill-1',
            runId: 'run-context-1',
            itemId: 'skill-review',
            itemKind: 'skill',
            position: 1,
            title: 'Review checklist',
            contentPreview: 'Check tests, deployment evidence, and issue notes.',
            contentTruncated: false,
            contentRef: 'skills/skill-review',
            scopeKind: 'org',
            scopeId: 'org-1',
            sensitivity: 'internal',
            state: 'active',
            revoked: false,
            sourceTaskId: null,
            sourceRunId: null,
            source: {
              sourceType: 'skill',
              sourceId: 'skill-review',
              title: 'Review checklist',
            },
            lastUsedAt: new Date(Date.now() - 300_000).toISOString(),
            lastVerifiedAt: null,
            appliedAt: new Date(Date.now() - 300_000).toISOString(),
            adapter: 'claude',
            envelopeVersion: 'v1',
            capabilityProfile: { cliTool: 'claude' },
            degradationReason: null,
            feedback: null,
          },
        ],
        suggestedMemoryUpdates: [],
        skillCandidates: [],
        evidence: [
          {
            runId: 'run-context-1',
            sourceType: 'task_result',
            sourceId: 'evidence-1',
            agentId: 'agent-t-003',
            payload: { ok: true },
            createdAt: new Date(Date.now() - 240_000).toISOString(),
          },
        ],
        provenance: [
          {
            runId: 'run-context-1',
            itemId: 'memory-prod-ext',
            itemKind: 'memory',
            title: 'Prod-ext validation memory',
            source: {
              sourceType: 'memory_item',
              sourceId: 'memory-prod-ext',
              title: 'Prod-ext validation memory',
            },
            adapter: 'claude',
            envelopeVersion: 'v1',
            appliedAt: new Date(Date.now() - 300_000).toISOString(),
            state: 'active',
            revoked: false,
          },
        ],
      },
    })
  )
  await context.route('**/api/v1/context/feedback', (route) =>
    json(route, {
      ok: true,
      data: {
        feedback: {
          id: 'feedback-1',
          organization_id: 'org-1',
          workspace_id: 'workspace-1',
          run_id: 'run-context-1',
          item_id: 'memory-prod-ext',
          item_kind: 'memory',
          label: 'useful',
          note: null,
          user_id: 'user-1',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
        item_state_changed: false,
      },
    })
  )
  await context.route('**/api/v1/context/previews', (route) =>
    json(route, {
      ok: true,
      data: {
        contextPreviewId: 'preview-e2e-1',
        previewHash: 'preview-hash-e2e',
        taskId: 't-001',
        agentId: 'agent-preview-1',
        expiresAt: new Date(Date.now() + 900_000).toISOString(),
        capability: {
          cli_tool: 'codex',
          runtime_kind: 'container',
          max_context_tokens: 1200,
        },
        degradation: ['budget_truncated'],
        items: [
          {
            id: 'memory-prod-ext',
            itemKind: 'memory',
            title: 'Prod-ext validation memory',
            selected: true,
            pinned: false,
            scopeKind: 'project',
            scopeId: 'proj-1',
            sensitivity: 'internal',
            estimatedTokens: 120,
            lastUsedAt: new Date(Date.now() - 300_000).toISOString(),
            lastVerifiedAt: new Date(Date.now() - 120_000).toISOString(),
            why: 'Matched task text.',
          },
          {
            id: 'memory-rollback',
            itemKind: 'memory',
            title: 'Rollback memory',
            selected: true,
            pinned: false,
            scopeKind: 'team',
            scopeId: 'team-1',
            sensitivity: 'confidential',
            estimatedTokens: 80,
            lastUsedAt: null,
            lastVerifiedAt: null,
            why: 'Recent useful feedback.',
          },
        ],
        suggestedItems: [
          {
            id: 'memory-pinned',
            itemKind: 'memory',
            title: 'Pinned migration note',
            selected: false,
            pinned: false,
            scopeKind: 'project',
            scopeId: 'proj-1',
            sensitivity: 'internal',
            estimatedTokens: 300,
            lastUsedAt: null,
            lastVerifiedAt: null,
            why: 'Outside the default context budget.',
          },
        ],
        previouslyPinned: [],
        warnings: [],
      },
    })
  )
  await context.route('**/api/v1/orchestration/tasks/*/publish-with-context', (route) =>
    json(route, {
      ok: true,
      task: {
        ...makeMockTask('t-001', 'Implement login flow', 'working', 'high', 0, 'Codex Preview'),
        assignedTo: 'agent-preview-1',
      },
    })
  )
  await context.route('**/api/v1/orchestration/tasks', (route) => {
    if (route.request().method() === 'POST') {
      return json(route, {
        ok: true,
        task: makeMockTask('t-new', 'Newly created task', 'backlog'),
      })
    }
    return route.continue()
  })

  // Agent API — read-only listing for the React agents route. Mutations keep
  // their existing real-backend coverage in dedicated integration suites.
  await context.route('**/api/v1/agents', (route) => {
    if (route.request().method() === 'GET') return json(route, { ok: true, agents: MOCK_AGENTS })
    return route.continue()
  })
}

// ── Per-test overrides ──────────────────────────────────────────────────────

/** Override the `/api/v1/orgs` handler for the current context. Shadows the
 * default single-org mock installed by `installStandardMocks` (Playwright
 * matches context routes in reverse registration order, so the later
 * handler wins and the base never runs while this override is in place). */
export async function overrideOrgs(
  context: BrowserContext,
  orgs: ReadonlyArray<MockOrg>
): Promise<void> {
  await context.route('**/api/v1/orgs', (route) => json(route, { ok: true, orgs }))
}

// Override the per-org teams endpoint for the current context.
export async function overrideTeams(
  context: BrowserContext,
  teams: ReadonlyArray<MockTeam>
): Promise<void> {
  await context.route('**/api/v1/orgs/*/teams', (route) => json(route, { ok: true, teams }))
}

export async function mockContextFeatures(
  context: BrowserContext,
  features: MockContextFeatureSnapshot
): Promise<void> {
  await context.route('**/api/v1/context/features', (route) =>
    json(route, { ok: true, data: features })
  )
}
