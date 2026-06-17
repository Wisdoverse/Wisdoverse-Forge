/**
 * Provider + Prompt Agent UX — E2E spec (#21)
 *
 * Covers the issue-21 UI plumbing end-to-end using the same mocked-API
 * pattern as react-app-smoke.spec.ts.  All backend HTTP calls are
 * intercepted by page.route(); no real server required.
 *
 * Tests:
 *   1. CreateAgentModal — "Provider + Prompt" radio reveals system-prompt textarea.
 *   2. CreateAgentModal submit — POST body contains lowercase provider + systemPrompt.
 *   3. Agent list — provider+prompt agent shows "Provider" badge (no cliTool).
 *   4. Chat tab — ChatComposer renders; Send disabled when empty.
 *   5. ChatComposer Cmd/Ctrl+Enter — fires POST /prompt with correct body.
 *   6. AgentConfigTab — loads existing systemPrompt; PATCH body captured on Save.
 */

import { test, expect, type Page, type Route } from '@playwright/test'

// ── Mock agent fixture ───────────────────────────────────────────────────────

const PROV_AGENT = {
  id: 'agent-prov-1',
  name: 'Provider Agent',
  cliTool: null,
  provider: 'anthropic',
  model: 'claude-sonnet-4-6',
  systemPrompt: 'old prompt',
  status: 'idle',
  cwd: null,
  containerId: null,
  tasksCompleted: 0,
  tasksInProgress: 0,
  successRate: 0,
}

// A configured LLM provider (the gateway link). Since #629 the create-agent
// provider dropdown is sourced from the configured providers in the settings
// store, keyed by the provider-config id — not a static provider-key list.
const PROV_CONFIG_ID = 'prov-cfg-anthropic-1'

// ── SSE body helpers ─────────────────────────────────────────────────────────

function makeSSEBody(): string {
  const msgId = 'msg-prov-test-1'
  const frames = [
    `data: ${JSON.stringify({ type: 'message_start', message: { id: msgId, role: 'assistant', content: [] } })}\n\n`,
    `data: ${JSON.stringify({ type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: 'pong' } })}\n\n`,
    `data: ${JSON.stringify({ type: 'message_stop' })}\n\n`,
  ]
  return frames.join('')
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async function injectAuth(page: Page, _baseURL: string): Promise<void> {
  // Auth itself is provided by Playwright `storageState` (see global-setup.ts).
  // This helper only primes nav preferences so the sidebar tree auto-expands.
  await page.addInitScript(() => {
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })
}

async function setupNavMocks(page: Page): Promise<void> {
  await page.route('**/api/v1/orgs', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        orgs: [{ id: 'org-1', name: 'Test Org', slug: 'test-org', plan: 'pro', role: 'admin' }],
      }),
    })
  )
  await page.route('**/api/v1/orgs/*/teams', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        teams: [
          {
            id: 'team-1',
            orgId: 'org-1',
            name: 'Engineering',
            slug: 'engineering',
            visibility: 'private',
            description: '',
          },
        ],
      }),
    })
  )
  await page.route('**/api/v1/teams/*/projects', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        projects: [
          {
            id: 'proj-1',
            teamId: 'team-1',
            name: 'Wisdoverse Forge',
            slug: 'agentforge',
            color: '#007AFF',
            description: 'Main project',
          },
        ],
      }),
    })
  )
  await page.route('**/api/v1/groups?*', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        groups: [{ id: 'grp-1', name: 'Default', projectId: 'proj-1' }],
      }),
    })
  )
  await page.route('**/api/v1/orchestration/**', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, tasks: [], stats: { byState: {} } }),
    })
  )
}

async function setupAgentMocks(page: Page, agents: object[] = [PROV_AGENT]): Promise<void> {
  // GET list
  await page.route('**/api/v1/agents', (r: Route) => {
    if (r.request().method() === 'GET') {
      return r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, agents }),
      })
    }
    // POST — handled by callers that need to capture body
    return r.continue()
  })
  // GET messages
  await page.route('**/api/v1/agents/*/messages', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, messages: [], hasMore: false }),
    })
  )
  // DELETE messages
  await page.route('**/api/v1/agents/*/messages', (r: Route) => {
    if (r.request().method() === 'DELETE') {
      return r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, deleted: 0 }),
      })
    }
    return r.continue()
  })
  // POST interrupt
  await page.route('**/api/v1/agents/*/prompt/interrupt', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true }),
    })
  )
  // PATCH agent (system prompt update)
  await page.route('**/api/v1/agents/*', (r: Route) => {
    if (r.request().method() === 'PATCH') {
      return r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, data: {} }),
      })
    }
    return r.continue()
  })
}

async function waitForAppReady(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded')
  await page.waitForFunction(
    () => {
      const root = document.getElementById('root')
      return root && root.children.length > 0
    },
    { timeout: 30000 }
  )
  await page.locator('[data-testid="sidebar"]').waitFor({ state: 'attached', timeout: 15000 })
}

// Seed the configured LLM providers (the gateway). The CreateAgentModal
// self-loads these on open via GET /llm-providers, so the Provider + Prompt
// dropdown is populated even when opened from a deep link to /agents.
async function setupProviderMocks(page: Page): Promise<void> {
  await page.route('**/api/v1/llm-providers', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        ok: true,
        providers: [
          {
            id: PROV_CONFIG_ID,
            provider: 'anthropic',
            displayName: 'Anthropic',
            model: 'claude-sonnet-4-6',
            isEnabled: true,
            lastTestStatus: 'passed',
          },
        ],
      }),
    })
  )
}

async function navigateToAgents(page: Page, baseURL: string): Promise<void> {
  await injectAuth(page, baseURL)
  await setupNavMocks(page)
  await setupAgentMocks(page)
  await setupProviderMocks(page)
  await page.goto(baseURL)
  await waitForAppReady(page)
  await page.locator('[data-testid="sidebar-nav-agents"]').click()
  await page.waitForURL('**/agents')
  await expect(page.getByRole('heading', { name: 'Agents' })).toBeVisible({ timeout: 5000 })
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe.serial('Provider + Prompt Agent UX (#21)', () => {
  // 1. CreateAgentModal — kind switch reveals system-prompt textarea ───────────

  test('1. Provider+Prompt radio reveals system-prompt textarea', async ({ page, baseURL }) => {
    await navigateToAgents(page, baseURL!)

    // Open modal
    await page.getByText('New Agent').first().click()
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 })

    // With a tested provider seeded, the modal defaults to Provider + Prompt
    // (buildDefaultValues picks the verified provider). The system-prompt
    // textarea is part of the provider UI, so this test exercises the toggle
    // default-independently: switch to Container CLI, then back to provider.
    const kindGroup = page.getByRole('radiogroup', { name: 'Agent kind' })

    // Container CLI kind hides the system-prompt textarea.
    await kindGroup.getByText('Container CLI').click()
    await expect(page.locator('textarea#systemPrompt')).not.toBeVisible()

    // Switching to Provider + Prompt reveals the system-prompt textarea.
    await kindGroup.getByText('Provider + Prompt').click()
    await expect(page.locator('textarea#systemPrompt')).toBeVisible({ timeout: 3000 })
    await expect(page.getByPlaceholder(/concise.*code reviewer/i)).toBeVisible()
  })

  // 2. Submit POST — body contains lowercase provider + systemPrompt ──────────

  test('2. Submit form sends lowercase provider and systemPrompt', async ({ page, baseURL }) => {
    await navigateToAgents(page, baseURL!)

    // Intercept the POST before opening the modal
    let capturedBody: Record<string, unknown> = {}
    await page.route('**/api/v1/agents', async (r: Route) => {
      if (r.request().method() === 'POST') {
        capturedBody = (await r.request().postDataJSON()) as Record<string, unknown>
        return r.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            ok: true,
            agent: {
              ...PROV_AGENT,
              id: 'agent-prov-2',
              name: 'My LLM Agent',
              systemPrompt: 'Be concise.',
            },
          }),
        })
      }
      return r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, agents: [PROV_AGENT] }),
      })
    })

    await page.getByText('New Agent').first().click()
    await expect(page.getByRole('dialog')).toBeVisible({ timeout: 5000 })

    // Switch to provider kind, then wait for the gateway providers to self-load
    // into the dropdown. The select is keyed by provider-config id (not the
    // static provider key). Waiting here also lets the provider-load form reset
    // settle before we fill the remaining fields, so nothing gets clobbered.
    await page
      .getByRole('radiogroup', { name: 'Agent kind' })
      .getByText('Provider + Prompt')
      .click()
    const providerSelect = page.locator('select#agent-provider')
    await expect(providerSelect).toHaveValue(PROV_CONFIG_ID, { timeout: 5000 })

    // Fill name + system prompt after the provider selection has settled
    await page.getByPlaceholder('e.g. Frontend Agent').fill('My LLM Agent')
    await page.locator('textarea#systemPrompt').fill('Be concise.')

    // Submit
    await page.getByRole('button', { name: 'Create Agent' }).click()

    // Wait for modal to close (store closes on success)
    await expect(page.getByRole('dialog')).not.toBeVisible({ timeout: 5000 })

    // Assert captured body
    expect(capturedBody.name).toBe('My LLM Agent')
    expect(capturedBody.provider).toBe('anthropic') // lowercase
    expect(capturedBody.systemPrompt).toBe('Be concise.')
  })

  // 3. Agent list renders provider badge when cliTool is null ────────────────

  test('3. Agent list shows Provider badge for provider+prompt agent', async ({
    page,
    baseURL,
  }) => {
    await navigateToAgents(page, baseURL!)

    // Agent card should be present
    const card = page.locator('[data-testid="agent-card-agent-prov-1"]')
    await expect(card).toBeVisible({ timeout: 5000 })

    // AgentKindBadge renders "Provider" when cliTool is absent
    await expect(card.getByText('Provider', { exact: true })).toBeVisible()

    // Should NOT have "Container" badge
    await expect(card.getByText('Container', { exact: true })).not.toBeVisible()
  })

  // 4. Chat tab — ChatComposer renders; Send disabled when empty ─────────────

  test('4. Chat tab renders composer; empty input does not send', async ({ page, baseURL }) => {
    await navigateToAgents(page, baseURL!)

    // Spy on the prompt endpoint so we can assert an empty Send is a no-op.
    let promptCalls = 0
    await page.route('**/api/v1/agents/*/prompt', (r: Route) => {
      promptCalls += 1
      return r.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true }),
      })
    })

    // Click the provider agent card
    await page.locator('[data-testid="agent-card-agent-prov-1"]').click()

    // Switch to the Chat/History tab (no containerId → label is "Chat")
    await page.getByRole('button', { name: 'Chat' }).click()

    // ChatComposer textarea should be visible
    const composer = page.getByPlaceholder(/Ask this agent/i)
    await expect(composer).toBeVisible({ timeout: 5000 })

    // The Send button guards empty input in its click handler (it trims and
    // returns), so clicking it with an empty composer must NOT fire a prompt.
    const sendBtn = page.getByRole('button', { name: 'Send' })
    await expect(sendBtn).toBeVisible()
    await sendBtn.click()
    await page.waitForTimeout(300)
    expect(promptCalls).toBe(0)
  })

  // 5. Cmd/Ctrl+Enter triggers POST /prompt ──────────────────────────────────

  test('5. Cmd/Ctrl+Enter sends prompt to /prompt endpoint', async ({ page, baseURL }) => {
    await navigateToAgents(page, baseURL!)

    // Intercept the prompt POST
    let promptBody: Record<string, unknown> = {}
    await page.route('**/api/v1/agents/*/prompt', async (r: Route) => {
      if (r.request().method() === 'POST') {
        promptBody = (await r.request().postDataJSON()) as Record<string, unknown>
        return r.fulfill({
          status: 200,
          contentType: 'text/event-stream',
          body: makeSSEBody(),
        })
      }
      return r.continue()
    })

    await page.locator('[data-testid="agent-card-agent-prov-1"]').click()
    await page.getByRole('button', { name: 'Chat' }).click()

    const composer = page.getByPlaceholder(/Ask this agent/i)
    await expect(composer).toBeVisible({ timeout: 5000 })
    await composer.fill('ping')

    // Send button now enabled
    const sendBtn = page.getByRole('button', { name: 'Send' })
    await expect(sendBtn).toBeEnabled()

    // Press Ctrl+Enter (Meta+Enter is cmd on macOS; Ctrl+Enter works cross-platform)
    await composer.press('Control+Enter')

    // Wait for the prompt request to arrive (route capture fires async)
    await page
      .waitForFunction(
        () => (window as unknown as Record<string, unknown>).__promptCaptured === true,
        { timeout: 5000 }
      )
      .catch(() => {
        // If the window flag isn't set, fall through — we assert the body below
      })

    // Give the route handler a moment to be called before asserting
    await page.waitForTimeout(500)

    // The content field should contain the typed message
    const content = (promptBody.content ?? promptBody.prompt) as string | undefined
    expect(content).toBe('ping')
  })

  // 6. AgentConfigTab loads existing systemPrompt and saves via PATCH ─────────

  test('6. AgentConfigTab loads existing systemPrompt and PATCHes on Save', async ({
    page,
    baseURL,
  }) => {
    await navigateToAgents(page, baseURL!)

    // Intercept the PATCH to capture its body; also intercept re-load GET
    let patchBody: Record<string, unknown> = {}
    await page.route('**/api/v1/agents/*', async (r: Route) => {
      if (r.request().method() === 'PATCH') {
        patchBody = (await r.request().postDataJSON()) as Record<string, unknown>
        return r.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ ok: true, data: {} }),
        })
      }
      if (r.request().method() === 'GET') {
        // Re-load agents after save
        return r.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ ok: true, agents: [PROV_AGENT] }),
        })
      }
      return r.continue()
    })

    await page.locator('[data-testid="agent-card-agent-prov-1"]').click()

    // Navigate to Config tab
    await page.getByRole('button', { name: 'Instructions' }).click()

    // Existing system prompt should be pre-filled
    const promptTextarea = page.locator('textarea#config-system-prompt')
    await expect(promptTextarea).toBeVisible({ timeout: 5000 })
    await expect(promptTextarea).toHaveValue('old prompt')

    // Edit the prompt
    await promptTextarea.fill('new prompt')

    // Save button should be enabled (dirty)
    const saveBtn = page.getByRole('button', { name: 'Save' })
    await expect(saveBtn).toBeEnabled()
    await saveBtn.click()

    // Wait for PATCH to be issued
    await page.waitForTimeout(500)

    // Assert PATCH body contained updated systemPrompt
    expect(patchBody.systemPrompt).toBe('new prompt')
  })
})
