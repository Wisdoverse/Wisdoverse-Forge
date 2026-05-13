import {
  expect,
  request as playwrightRequest,
  test,
  type APIRequestContext,
  type Page,
} from '@playwright/test'
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import crypto from 'node:crypto'
import { chmod, mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { Client as PgClient } from 'pg'
import { connect as connectNats } from 'nats'

const here = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(here, '../../..')
const rustRoot = path.join(repoRoot, 'rust')

const DEFAULT_LOCAL_PASSWORD = 'DevPass123!'
const STABLE_E2E_EMAIL = 'dev@example.com'
const REAL_E2E_ENABLED = process.env.ORCHESTRATION_REAL_E2E === '1'
const REAL_CLI_E2E = process.env.ORCHESTRATION_REAL_CLI_E2E === '1'
const REAL_CLI_TOOL = process.env.ORCHESTRATION_REAL_CLI_TOOL ?? 'codex'
const REAL_CLI_MODEL = process.env.ORCHESTRATION_REAL_CLI_MODEL?.trim() || undefined
const DATABASE_URL = configuredDatabaseUrl()
const SIDECAR_BIN =
  process.env.AGENTFORGE_SIDECAR_BIN ?? path.join(rustRoot, 'target/debug/agentforge-sidecar')
const SIDECAR_CONTAINER_IMAGE = process.env.AGENTFORGE_SIDECAR_CONTAINER_IMAGE?.trim() || undefined
const SIDECAR_CONTAINER_NATS_HOST =
  process.env.AGENTFORGE_SIDECAR_CONTAINER_NATS_HOST ?? 'host.docker.internal'
const NATS_PORT = process.env.NATS_PORT ?? '4222'
const SIDECAR_START_TIMEOUT_MS = positiveIntEnv(
  process.env.ORCHESTRATION_REAL_SIDECAR_START_TIMEOUT_MS,
  60_000
)
const REAL_CLI_TASK_TIMEOUT_MS = positiveIntEnv(
  process.env.ORCHESTRATION_REAL_CLI_TASK_TIMEOUT_MS,
  REAL_CLI_E2E ? 300_000 : 45_000
)
const REAL_E2E_TEST_TIMEOUT_MS = positiveIntEnv(
  process.env.ORCHESTRATION_REAL_E2E_TEST_TIMEOUT_MS,
  REAL_CLI_E2E ? SIDECAR_START_TIMEOUT_MS + REAL_CLI_TASK_TIMEOUT_MS + 60_000 : 120_000
)
const MAX_DIAGNOSTIC_ENTRIES = 40
const MAX_DIAGNOSTIC_CHARS = 2_000

if (REAL_E2E_ENABLED) {
  validateRealE2ESafety()
}

interface LoginResponse {
  ok: boolean
  user: {
    id: string
    email: string
    orgId: string
    role: string
  }
  tokens: {
    accessToken: string
  }
}

interface SwitchContextResponse {
  ok: boolean
  accessToken: string
}

interface TaskSummary {
  id: string
  state: string
  assignedTo?: string
  params?: { task?: string }
  result?: { stdout?: string }
}

interface TestFixture {
  api: APIRequestContext
  db: PgClient
  email: string
  token: string
  userId: string
  orgId: string
  workspaceId: string
  teamId: string
  projectId: string
  groupId: string
  agentId: string
  agentName: string
  hmacSecret: string
  natsPassword: string
  cliTool: string
  cliWorkDir: string
  walPath: string
  fakeBinDir: string
  sidecarLogs: string[]
  taskIds: string[]
}

interface PageDiagnostics {
  console: string[]
  pageErrors: string[]
  requestFailures: string[]
  badResponses: string[]
}

interface RunningSidecar {
  process: ChildProcessWithoutNullStreams
  containerName?: string
}

function uuid(): string {
  return crypto.randomUUID()
}

function assignmentConsumerName(agentId: string): string {
  return `orch-assignment-${agentId.replaceAll('-', '')}`
}

function boundedPush(items: string[], item: string) {
  items.push(
    item.length > MAX_DIAGNOSTIC_CHARS
      ? `${item.slice(0, MAX_DIAGNOSTIC_CHARS)}... [truncated]`
      : item
  )
  if (items.length > MAX_DIAGNOSTIC_ENTRIES) {
    items.splice(0, items.length - MAX_DIAGNOSTIC_ENTRIES)
  }
}

function requiredRealE2EEnv(name: string): string {
  const value = process.env[name]
  if (!value) {
    throw new Error(`${name} is required when ORCHESTRATION_REAL_E2E=1`)
  }
  return value
}

function positiveIntEnv(value: string | undefined, fallback: number): number {
  if (!value) return fallback
  const parsed = Number.parseInt(value, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

function configuredDatabaseUrl(): string {
  if (REAL_E2E_ENABLED) return requiredRealE2EEnv('E2E_DATABASE_URL')
  return (
    process.env.E2E_DATABASE_URL ??
    process.env.DATABASE_URL ??
    ['postgres://agentforge:', 'devpassword', '@127.0.0.1:45432/agentforge'].join('')
  )
}

function testEmail(): string {
  if (REAL_E2E_ENABLED) return requiredRealE2EEnv('E2E_EMAIL')
  return process.env.E2E_EMAIL ?? STABLE_E2E_EMAIL
}

function testPassword(): string {
  if (REAL_E2E_ENABLED) return requiredRealE2EEnv('E2E_PASSWORD')
  return process.env.E2E_PASSWORD ?? DEFAULT_LOCAL_PASSWORD
}

function validateRealE2ESafety() {
  const email = requiredRealE2EEnv('E2E_EMAIL')
  requiredRealE2EEnv('E2E_PASSWORD')
  requiredRealE2EEnv('E2E_DATABASE_URL')

  if (process.env.ORCHESTRATION_REAL_E2E_CLEANUP_AUTH !== '1') {
    throw new Error(
      'ORCHESTRATION_REAL_E2E_CLEANUP_AUTH=1 is required for real orchestration E2E cleanup'
    )
  }
  const normalizedEmail = email.toLowerCase()
  if (normalizedEmail !== STABLE_E2E_EMAIL && !normalizedEmail.includes('e2e')) {
    throw new Error(
      `E2E_EMAIL must be ${STABLE_E2E_EMAIL} or a disposable account containing "e2e"`
    )
  }
  if (REAL_CLI_E2E) {
    requiredRealE2EEnv('ORCHESTRATION_REAL_CLI_HOME')
    if (!['claude', 'codex', 'gemini', 'opencode'].includes(REAL_CLI_TOOL)) {
      throw new Error(
        'ORCHESTRATION_REAL_CLI_TOOL must be "codex", "claude", "gemini", or "opencode" when ORCHESTRATION_REAL_CLI_E2E=1'
      )
    }
  }
}

function shouldDeleteAuthAccount(email: string): boolean {
  return email.toLowerCase() !== STABLE_E2E_EMAIL
}

function credentialDirForTool(cliTool: string): string {
  switch (cliTool) {
    case 'codex':
      return '.codex'
    case 'claude':
      return '.claude'
    case 'gemini':
      return '.gemini'
    case 'opencode':
      return '.local/share/opencode'
    default:
      throw new Error(`real CLI E2E only supports codex/claude/gemini/opencode, got ${cliTool}`)
  }
}

async function login(
  baseURL: string
): Promise<{ api: APIRequestContext; body: LoginResponse; email: string }> {
  const email = testEmail()
  const password = testPassword()
  const api = await playwrightRequest.newContext({ baseURL })

  const register = await api.post('/api/v1/auth/register', {
    data: { email, password, username: 'dev' },
    failOnStatusCode: false,
  })
  if (![201, 409].includes(register.status())) {
    const body = await register.text()
    await api.dispose()
    throw new Error(`register failed: ${register.status()} ${body.slice(0, 300)}`)
  }

  const response = await api.post('/api/v1/auth/login', {
    data: { email, password },
  })
  const body = (await response.json()) as LoginResponse
  if (!body.ok || !body.user.orgId || !body.tokens.accessToken) {
    await api.dispose()
    throw new Error(`login did not return org/token: ${JSON.stringify(body)}`)
  }

  return { api, body, email }
}

async function seedFixture(baseURL: string): Promise<TestFixture> {
  const { api, body, email } = await login(baseURL)
  const db = new PgClient({ connectionString: DATABASE_URL })
  await db.connect()

  const suffix = Date.now().toString(36)
  const workspaceId = uuid()
  const teamId = uuid()
  const projectId = uuid()
  const groupId = uuid()
  const agentId = uuid()
  const hmacSecret = `e2e-hmac-${uuid()}`
  const natsPassword = uuid()
  const agentName = `E2E Worker ${suffix}`
  const fixtureRoot = path.join(repoRoot, 'test-results', 'orchestration-e2e', suffix)
  const fakeBinDir = path.join(fixtureRoot, 'bin')
  const cliWorkDir = path.join(fixtureRoot, 'workspace')
  const walPath = path.join(fixtureRoot, 'wal')
  const cliTool = REAL_CLI_E2E ? REAL_CLI_TOOL : 'codex'

  await mkdir(fakeBinDir, { recursive: true })
  await mkdir(cliWorkDir, { recursive: true })
  await mkdir(walPath, { recursive: true })
  if (!REAL_CLI_E2E) {
    const fakeCodex = path.join(fakeBinDir, 'codex')
    await writeFile(
      fakeCodex,
      [
        '#!/usr/bin/env bash',
        'set -euo pipefail',
        'if [[ "${1:-}" != "exec" ]]; then',
        '  echo "unexpected codex invocation: $*" >&2',
        '  exit 2',
        'fi',
        'prompt="${@: -1}"',
        'echo "E2E sidecar completed: ${prompt}"',
      ].join('\n')
    )
    await chmod(fakeCodex, 0o755)
  }

  await db.query('BEGIN')
  try {
    await db.query('INSERT INTO workspaces (id, organization_id, name) VALUES ($1, $2, $3)', [
      workspaceId,
      body.user.orgId,
      `E2E Workspace ${suffix}`,
    ])
    await db.query(
      `INSERT INTO teams (id, organization_id, name, slug, visibility, description)
       VALUES ($1, $2, $3, $4, 'private', $5)`,
      [
        teamId,
        body.user.orgId,
        `E2E Team ${suffix}`,
        `e2e-team-${suffix}`,
        'real orchestration E2E',
      ]
    )
    await db.query(
      `INSERT INTO projects (id, organization_id, workspace_id, team_id, name, slug, color, description)
       VALUES ($1, $2, $3, $4, $5, $6, '#0A84FF', $7)`,
      [
        projectId,
        body.user.orgId,
        workspaceId,
        teamId,
        `E2E Project ${suffix}`,
        `e2e-project-${suffix}`,
        'real orchestration E2E',
      ]
    )
    await db.query(
      `INSERT INTO groups (id, organization_id, name, description, created_by, project_id)
       VALUES ($1, $2, $3, $4, $5, $6)`,
      [groupId, body.user.orgId, `E2E Board ${suffix}`, 'real task board', body.user.id, projectId]
    )
    await db.query(
      `INSERT INTO group_members (group_id, user_id, role)
       VALUES ($1, $2, 'admin')
       ON CONFLICT (group_id, user_id) DO UPDATE SET role = EXCLUDED.role`,
      [groupId, body.user.id]
    )
    await db.query(
      `INSERT INTO team_members (team_id, user_id, role)
       VALUES ($1, $2, 'admin')
       ON CONFLICT (team_id, user_id) DO UPDATE SET role = EXCLUDED.role`,
      [teamId, body.user.id]
    )
    await db.query(
      `INSERT INTO project_members (project_id, user_id, role)
       VALUES ($1, $2, 'admin')
       ON CONFLICT (project_id, user_id) DO UPDATE SET role = EXCLUDED.role`,
      [projectId, body.user.id]
    )
    await db.query(
      `INSERT INTO agents
         (id, organization_id, workspace_id, project_id, user_id, name, status, model, provider, cli_tool,
          hmac_secret, nats_connect_password, cwd, runtime_id, last_activity_at)
       VALUES ($1, $2, $3, $4, $5, $6, 'idle', $7, $8, $9,
          $10, $11, $12, $13, NOW())`,
      [
        agentId,
        body.user.orgId,
        workspaceId,
        projectId,
        body.user.id,
        agentName,
        `agentforge-agent:${cliTool}`,
        cliTool,
        cliTool,
        hmacSecret,
        natsPassword,
        cliWorkDir,
        `e2e-${suffix}`,
      ]
    )
    await db.query(
      `INSERT INTO participants (organization_id, agent_id, name, capabilities, status, last_heartbeat_at)
       VALUES ($1, $2, $3, ARRAY[$4]::text[], 'available', NOW())
       ON CONFLICT (organization_id, agent_id) DO UPDATE
          SET name = EXCLUDED.name,
              capabilities = EXCLUDED.capabilities,
              status = 'available',
              last_heartbeat_at = NOW()`,
      [body.user.orgId, agentId, agentName, cliTool]
    )
    await db.query('COMMIT')
  } catch (error) {
    await db.query('ROLLBACK')
    await db.end()
    await api.dispose()
    throw error
  }

  const scopedToken = await switchContextToken(api, body.tokens.accessToken, {
    orgId: body.user.orgId,
    workspaceId,
    teamId,
    projectId,
  })

  return {
    api,
    db,
    email,
    token: scopedToken,
    userId: body.user.id,
    orgId: body.user.orgId,
    workspaceId,
    teamId,
    projectId,
    groupId,
    agentId,
    agentName,
    hmacSecret,
    natsPassword,
    cliTool,
    cliWorkDir,
    walPath,
    fakeBinDir,
    sidecarLogs: [],
    taskIds: [],
  }
}

async function switchContextToken(
  api: APIRequestContext,
  token: string,
  context: { orgId: string; workspaceId: string; teamId: string; projectId: string }
): Promise<string> {
  const response = await api.post('/api/v1/auth/switch-context', {
    headers: { Authorization: `Bearer ${token}` },
    data: context,
  })
  const body = (await response.json()) as SwitchContextResponse
  if (!body.ok || !body.accessToken) {
    throw new Error(`switch-context did not return a scoped token: ${JSON.stringify(body)}`)
  }
  return body.accessToken
}

async function cleanupFixture(fixture?: TestFixture) {
  if (!fixture) return
  await deleteAssignmentConsumer(fixture.agentId).catch(() => undefined)
  await fixture.db.query('BEGIN')
  try {
    await fixture.db.query('DELETE FROM orchestration_inbox WHERE task_id = ANY($1::uuid[])', [
      fixture.taskIds,
    ])
    await fixture.db.query(
      'DELETE FROM orchestration_outbox WHERE aggregate_id = ANY($1::uuid[])',
      [fixture.taskIds]
    )
    await fixture.db.query('DELETE FROM orchestration_tasks WHERE group_id = $1', [fixture.groupId])
    await fixture.db.query(
      'DELETE FROM participants WHERE organization_id = $1 AND agent_id = $2',
      [fixture.orgId, fixture.agentId]
    )
    await fixture.db.query('DELETE FROM agents WHERE id = $1', [fixture.agentId])
    await fixture.db.query('DELETE FROM group_members WHERE group_id = $1', [fixture.groupId])
    await fixture.db.query('DELETE FROM groups WHERE id = $1', [fixture.groupId])
    await fixture.db.query('DELETE FROM projects WHERE id = $1', [fixture.projectId])
    await fixture.db.query('DELETE FROM teams WHERE id = $1', [fixture.teamId])
    await fixture.db.query('DELETE FROM workspaces WHERE id = $1', [fixture.workspaceId])
    if (
      process.env.ORCHESTRATION_REAL_E2E_CLEANUP_AUTH === '1' &&
      shouldDeleteAuthAccount(fixture.email)
    ) {
      await fixture.db.query(
        'DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2',
        [fixture.orgId, fixture.userId]
      )
      await fixture.db.query(
        `DELETE FROM organizations
          WHERE id = $1
            AND NOT EXISTS (
              SELECT 1 FROM organization_members WHERE organization_id = $1
            )`,
        [fixture.orgId]
      )
      await fixture.db.query('DELETE FROM users WHERE id = $1 AND email = $2', [
        fixture.userId,
        fixture.email,
      ])
    }
    await fixture.db.query('COMMIT')
  } catch (error) {
    await fixture.db.query('ROLLBACK')
    throw error
  } finally {
    await fixture.db.end()
    await fixture.api.dispose()
    await rm(path.dirname(fixture.fakeBinDir), { recursive: true, force: true }).catch(
      () => undefined
    )
  }
}

async function deleteAssignmentConsumer(agentId: string) {
  const password = await dockerEnv('NATS_BACKEND_PASSWORD')
  if (!password) return
  const nc = await connectNats({
    servers: `nats://127.0.0.1:${NATS_PORT}`,
    user: 'backend',
    pass: password,
  })
  try {
    const jsm = await nc.jetstreamManager()
    await jsm.consumers.delete('ORCHESTRATION_ASSIGNMENTS', assignmentConsumerName(agentId))
  } finally {
    await nc.drain()
  }
}

async function dockerEnv(key: string): Promise<string | null> {
  const contents = await readFile(path.join(repoRoot, 'docker/.env'), 'utf8').catch(() => '')
  for (const line of contents.split('\n')) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    const [name, ...rest] = trimmed.split('=')
    if (name === key) return rest.join('=')
  }
  return null
}

function attachPageDiagnostics(page: Page): PageDiagnostics {
  const diagnostics: PageDiagnostics = {
    console: [],
    pageErrors: [],
    requestFailures: [],
    badResponses: [],
  }

  page.on('console', (message) => {
    boundedPush(diagnostics.console, `${message.type()}: ${message.text()}`)
  })
  page.on('pageerror', (error) => {
    boundedPush(diagnostics.pageErrors, error.stack ?? error.message)
  })
  page.on('requestfailed', (request) => {
    boundedPush(
      diagnostics.requestFailures,
      `${request.method()} ${request.url()} :: ${request.failure()?.errorText ?? 'unknown'}`
    )
  })
  page.on('response', (response) => {
    if (response.status() >= 400) {
      boundedPush(diagnostics.badResponses, `${response.status()} ${response.url()}`)
    }
  })

  return diagnostics
}

async function isBlankRoot(page: Page): Promise<boolean> {
  return page
    .locator('#root')
    .evaluate(
      (root) => root.childElementCount === 0 && (root.textContent ?? '').trim().length === 0
    )
    .catch(() => false)
}

async function pageDiagnosticSnapshot(page: Page, diagnostics: PageDiagnostics): Promise<string> {
  const [title, rootText, rootHtml] = await Promise.all([
    page.title().catch((error) => `title unavailable: ${String(error)}`),
    page
      .locator('#root')
      .evaluate(
        (root, maxChars) => (root.textContent ?? '').trim().slice(0, maxChars),
        MAX_DIAGNOSTIC_CHARS
      )
      .catch((error) => `root text unavailable: ${String(error)}`),
    page
      .locator('#root')
      .evaluate((root, maxChars) => root.innerHTML.slice(0, maxChars), MAX_DIAGNOSTIC_CHARS)
      .catch((error) => `root html unavailable: ${String(error)}`),
  ])

  return [
    `url=${page.url()}`,
    `title=${title}`,
    `rootText=${rootText || '<empty>'}`,
    `rootHtml=${rootHtml || '<empty>'}`,
    `console=${JSON.stringify(diagnostics.console)}`,
    `pageErrors=${JSON.stringify(diagnostics.pageErrors)}`,
    `requestFailures=${JSON.stringify(diagnostics.requestFailures)}`,
    `badResponses=${JSON.stringify(diagnostics.badResponses)}`,
  ].join('\n')
}

async function gotoTasksWithDiagnostics(page: Page, diagnostics: PageDiagnostics) {
  let lastError: unknown
  for (let attempt = 1; attempt <= 2; attempt += 1) {
    await page.goto('/tasks', { waitUntil: 'domcontentloaded' })
    try {
      await page.locator('[data-testid="sidebar"]').waitFor({ state: 'visible', timeout: 15_000 })
      return
    } catch (error) {
      lastError = error
      if (attempt === 1 && (await isBlankRoot(page))) {
        continue
      }
    }
  }

  throw new Error(
    `sidebar did not become visible after navigating to /tasks: ${String(lastError)}\n${await pageDiagnosticSnapshot(
      page,
      diagnostics
    )}`
  )
}

async function enrichFailure(
  error: unknown,
  page: Page,
  diagnostics: PageDiagnostics,
  fixture?: TestFixture
): Promise<Error> {
  const message = error instanceof Error ? (error.stack ?? error.message) : String(error)
  const sidecarLogs = fixture?.sidecarLogs.length
    ? fixture.sidecarLogs.slice(-MAX_DIAGNOSTIC_ENTRIES).join('')
    : '<empty>'
  return new Error(
    `${message}\n\nPage diagnostics:\n${await pageDiagnosticSnapshot(page, diagnostics)}\n\nSidecar logs:\n${sidecarLogs}`
  )
}

function startSidecar(fixture: TestFixture): Promise<RunningSidecar> {
  if (SIDECAR_CONTAINER_IMAGE) {
    return startContainerSidecar(fixture, SIDECAR_CONTAINER_IMAGE)
  }

  const natsUrl = `nats://${fixture.agentId}:${fixture.natsPassword}@127.0.0.1:${NATS_PORT}`
  const realCliHome = process.env.ORCHESTRATION_REAL_CLI_HOME
  const childEnv = {
    ...process.env,
    PATH: REAL_CLI_E2E
      ? (process.env.PATH ?? '')
      : `${fixture.fakeBinDir}:${process.env.PATH ?? ''}`,
    NATS_URL: natsUrl,
    AGENT_ID: fixture.agentId,
    HMAC_SECRET: fixture.hmacSecret,
    WAL_PATH: fixture.walPath,
    CLI_TOOL: fixture.cliTool,
    AGENTFORGE_CLI_TOOL: fixture.cliTool,
    ...(REAL_CLI_MODEL ? { AGENTFORGE_CLI_MODEL: REAL_CLI_MODEL } : {}),
    HEARTBEAT_INTERVAL_SECS: '1',
    RUST_LOG: 'info',
    ...(REAL_CLI_E2E && realCliHome
      ? {
          HOME: realCliHome,
          CODEX_HOME: path.join(realCliHome, '.codex'),
          CLAUDE_CONFIG_DIR: path.join(realCliHome, '.claude'),
          GEMINI_CONFIG_DIR: path.join(realCliHome, '.gemini'),
          GEMINI_CLI_NO_RELAUNCH: 'true',
          GEMINI_CLI_TRUST_WORKSPACE: 'true',
          XDG_CONFIG_HOME: path.join(realCliHome, '.config'),
          XDG_DATA_HOME: path.join(realCliHome, '.local/share'),
        }
      : {}),
  }
  const child = spawn(SIDECAR_BIN, [], {
    cwd: fixture.cliWorkDir,
    env: childEnv,
  })

  return waitForSidecarReady(child, fixture)
}

function startContainerSidecar(fixture: TestFixture, image: string): Promise<RunningSidecar> {
  const realCliHome = requiredRealE2EEnv('ORCHESTRATION_REAL_CLI_HOME')
  const credentialDir = credentialDirForTool(fixture.cliTool)
  const natsUrl = `nats://${fixture.agentId}:${fixture.natsPassword}@${SIDECAR_CONTAINER_NATS_HOST}:${NATS_PORT}`
  const containerName = `agentforge-e2e-sidecar-${fixture.agentId}`
  const env = {
    NATS_URL: natsUrl,
    AGENTFORGE_NATS_URL: natsUrl,
    AGENT_ID: fixture.agentId,
    HMAC_SECRET: fixture.hmacSecret,
    WAL_PATH: '/tmp/agentforge-wal',
    CLI_TOOL: fixture.cliTool,
    AGENTFORGE_CLI_TOOL: fixture.cliTool,
    ...(REAL_CLI_MODEL ? { AGENTFORGE_CLI_MODEL: REAL_CLI_MODEL } : {}),
    HEARTBEAT_INTERVAL_SECS: '1',
    RUST_LOG: 'info',
  }
  const envArgs = Object.entries(env).flatMap(([key, value]) => ['-e', `${key}=${value}`])
  const bootstrap = [
    'set -Eeuo pipefail',
    'install -d -o agent -g agent -m 700 /home/agent/.codex /home/agent/.claude /home/agent/.gemini /home/agent/.local/share/opencode /home/agent/.config/opencode /tmp/agentforge-wal /workspace',
    `if [ ! -d /run/agentforge-real-cli-home/${credentialDir} ]; then echo "missing ${credentialDir} credentials" >&2; exit 2; fi`,
    `cp -a /run/agentforge-real-cli-home/${credentialDir}/. /home/agent/${credentialDir}/`,
    'if [ -f /run/agentforge-real-cli-home/.claude.json ]; then cp -a /run/agentforge-real-cli-home/.claude.json /home/agent/.claude.json; fi',
    `chown -R agent:agent /home/agent/${credentialDir} /home/agent/.claude.json /tmp/agentforge-wal /workspace 2>/dev/null || chown -R agent:agent /home/agent/${credentialDir} /tmp/agentforge-wal /workspace`,
    [
      'exec setpriv --reuid=agent --regid=agent --init-groups env',
      'HOME=/home/agent',
      'PATH=/home/agent/.npm-global/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
      'CODEX_HOME=/home/agent/.codex',
      'CLAUDE_CONFIG_DIR=/home/agent/.claude',
      'GEMINI_CONFIG_DIR=/home/agent/.gemini',
      'GEMINI_CLI_NO_RELAUNCH=true',
      'GEMINI_CLI_TRUST_WORKSPACE=true',
      'XDG_CONFIG_HOME=/home/agent/.config',
      'XDG_DATA_HOME=/home/agent/.local/share',
      '/usr/local/bin/agentforge-sidecar',
    ].join(' '),
  ].join('\n')
  const args = [
    'run',
    '--rm',
    '--pull=never',
    '--name',
    containerName,
    '--label',
    'com.agentforge.e2e=orchestration-real-cli',
    '--add-host',
    `${SIDECAR_CONTAINER_NATS_HOST}:host-gateway`,
    '--user',
    '0:0',
    '--tmpfs',
    '/workspace:rw,nosuid,nodev,size=64m',
    '--tmpfs',
    '/tmp/agentforge-wal:rw,nosuid,nodev,size=64m',
    '-v',
    `${realCliHome}:/run/agentforge-real-cli-home:ro`,
    ...envArgs,
    '--entrypoint',
    '/bin/bash',
    image,
    '-lc',
    bootstrap,
  ]
  const child = spawn('docker', args, {
    cwd: repoRoot,
    env: process.env,
  })

  return waitForSidecarReady(child, fixture, containerName)
}

function waitForSidecarReady(
  child: ChildProcessWithoutNullStreams,
  fixture: TestFixture,
  containerName?: string
): Promise<RunningSidecar> {
  let logs = ''
  return new Promise((resolve, reject) => {
    let settled = false
    const cleanupFailedStart = () => {
      if (containerName) void stopSidecarContainer(containerName)
      if (!child.killed) child.kill('SIGKILL')
    }
    const timeout = setTimeout(() => {
      settled = true
      cleanupFailedStart()
      reject(
        new Error(
          `sidecar did not start orchestration subscriber within ${SIDECAR_START_TIMEOUT_MS}ms. Logs:\n${logs}`
        )
      )
    }, SIDECAR_START_TIMEOUT_MS)
    const fail = (error: Error) => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      cleanupFailedStart()
      reject(error)
    }
    const onData = (chunk: Buffer) => {
      const text = chunk.toString()
      logs += text
      boundedPush(fixture.sidecarLogs, text)
      if (logs.includes('Orchestration subscriber listening')) {
        if (settled) return
        settled = true
        clearTimeout(timeout)
        resolve({ process: child, containerName })
      }
    }
    child.stdout.on('data', onData)
    child.stderr.on('data', onData)
    child.once('error', (error) => {
      fail(error)
    })
    child.once('exit', (code, signal) => {
      fail(
        new Error(
          `sidecar exited before subscriber was ready: code=${code} signal=${signal}\n${logs}`
        )
      )
    })
  })
}

async function stopSidecar(sidecar?: RunningSidecar) {
  if (!sidecar) return
  const child = sidecar.process
  if (sidecar.containerName) {
    await stopSidecarContainer(sidecar.containerName)
  }
  if (child.killed) return
  child.kill('SIGINT')
  await new Promise<void>((resolve) => {
    const timeout = setTimeout(() => {
      child.kill('SIGKILL')
      resolve()
    }, 5_000)
    child.once('exit', () => {
      clearTimeout(timeout)
      resolve()
    })
  })
}

async function stopSidecarContainer(containerName: string): Promise<void> {
  await new Promise<void>((resolve) => {
    const child = spawn('docker', ['rm', '-f', containerName], {
      cwd: repoRoot,
      env: process.env,
    })
    child.once('exit', () => resolve())
    child.once('error', () => resolve())
  })
}

async function seedNavigation(page: Page, fixture: TestFixture) {
  await page.addInitScript(({ orgId, teamId, projectId, token }) => {
    localStorage.setItem('af:auth:access', token)
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', orgId)
    localStorage.setItem('af:nav:projectId', projectId)
    localStorage.setItem('af:nav:expandedTeams', JSON.stringify([teamId]))
  }, fixture)
}

async function waitForCompletedTask(fixture: TestFixture, title: string): Promise<TaskSummary> {
  const deadline = Date.now() + REAL_CLI_TASK_TIMEOUT_MS
  let lastBody = ''
  while (Date.now() < deadline) {
    const response = await fixture.api.get(
      `/api/v1/orchestration/groups/${fixture.groupId}/tasks`,
      {
        headers: { Authorization: `Bearer ${fixture.token}` },
      }
    )
    lastBody = await response.text()
    if (!response.ok()) {
      throw new Error(`task poll failed: ${response.status()} ${lastBody.slice(0, 500)}`)
    }
    const json = JSON.parse(lastBody) as { tasks?: TaskSummary[] }
    const task = json.tasks?.find((candidate) => candidate.params?.task === title)
    if (task?.id && !fixture.taskIds.includes(task.id)) fixture.taskIds.push(task.id)
    if (task?.state === 'completed') return task
    if (task?.state === 'failed') {
      throw new Error(`task failed before completion: ${JSON.stringify(task)}`)
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000))
  }
  throw new Error(`task did not complete in time. Last response: ${lastBody}`)
}

test.describe.configure({ mode: 'serial' })

test.describe('real orchestration task E2E', () => {
  test.skip(!REAL_E2E_ENABLED, 'set ORCHESTRATION_REAL_E2E=1 to mutate a real deployment')
  test.setTimeout(REAL_E2E_TEST_TIMEOUT_MS)

  let fixture: TestFixture | undefined
  let sidecar: RunningSidecar | undefined

  test.afterEach(async () => {
    await stopSidecar(sidecar)
    sidecar = undefined
  })

  test.afterAll(async () => {
    await cleanupFixture(fixture)
  })

  test('kanban publishes an assigned task and a real sidecar completes it', async ({
    page,
    baseURL,
  }) => {
    const diagnostics = attachPageDiagnostics(page)
    const target = baseURL ?? 'http://127.0.0.1:4003'
    try {
      fixture = await seedFixture(target)
      sidecar = await startSidecar(fixture)
      await seedNavigation(page, fixture)

      const realCliToken = `AGENTFORGE_REAL_CLI_OK_${Date.now()}`
      const title = REAL_CLI_E2E
        ? `Real CLI smoke ${realCliToken}`
        : `Real sidecar task ${Date.now()}`
      const details = REAL_CLI_E2E
        ? `You are running in a temporary empty workspace. Do not create, modify, or delete files. Print exactly ${realCliToken} and no extra commentary.`
        : 'Complete this deterministic E2E task.'
      await gotoTasksWithDiagnostics(page, diagnostics)
      await page.locator(`[data-testid="project-${fixture.projectId}"]`).click()
      await expect(page.locator('[data-testid="page-tasks"]')).toBeVisible()
      await expect(page.locator('[data-testid="column-count-backlog"]')).toBeVisible()

      await page.getByRole('button', { name: '+ Task' }).click()
      const dialog = page.getByRole('dialog', { name: 'New Task' })
      await expect(dialog).toBeVisible()
      await dialog.getByLabel('Title').fill(title)
      await dialog.getByLabel('Description').fill(details)
      await dialog.getByLabel('Assign Agent').selectOption(fixture.agentId)
      await dialog.getByRole('button', { name: 'Create Task' }).click()
      await expect(dialog).toBeHidden()

      const completed = await waitForCompletedTask(fixture, title)
      expect(completed.assignedTo).toBe(fixture.agentId)
      expect(completed.result?.stdout).toContain(
        REAL_CLI_E2E ? realCliToken : 'E2E sidecar completed'
      )

      await page.reload()
      await expect(page.locator(`[data-testid="task-card-${completed.id}"]`)).toContainText(title)
      await expect(page.locator(`[data-testid="task-card-${completed.id}"]`)).toContainText(
        '1 file'
      )
    } catch (error) {
      throw await enrichFailure(error, page, diagnostics, fixture)
    }
  })
})
