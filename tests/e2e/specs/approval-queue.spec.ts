import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

interface E2eCandidate {
  id: string
  workspace_id: string
  item_kind: 'memory' | 'skill'
  state: 'pending' | 'approved' | 'rejected' | 'superseded'
  owner_user_id: string
  source_run_id: string | null
  target_skill_id: string | null
  proposed_scope_kind: 'user' | 'team' | 'project' | 'org'
  source_available: boolean
  proposed_preview: Record<string, unknown>
  created_at: string
  updated_at: string
}

async function setupAndNavigate(page: Page, baseURL: string, candidates: E2eCandidate[]) {
  await page.addInitScript(() => {
    class MockWebSocket {
      static instances: MockWebSocket[] = []
      onopen: (() => void) | null = null
      onmessage: ((event: { data: string }) => void) | null = null
      onclose: (() => void) | null = null
      onerror: (() => void) | null = null
      readyState = 1

      constructor() {
        MockWebSocket.instances.push(this)
        window.setTimeout(() => this.onopen?.(), 0)
      }

      send() {}

      close() {
        this.readyState = 3
        this.onclose?.()
      }
    }

    Object.defineProperty(window, 'WebSocket', {
      value: MockWebSocket,
      configurable: true,
      writable: true,
    })
    Object.defineProperty(window, '__afEmitWs', {
      value: (payload: unknown) => {
        for (const socket of MockWebSocket.instances) {
          socket.onmessage?.({ data: JSON.stringify(payload) })
        }
      },
      configurable: true,
    })

    const payload = btoa(
      JSON.stringify({ sub: 'user-1', exp: Math.floor(Date.now() / 1000) + 3600 })
    )
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/g, '')
    localStorage.setItem('af:auth:access', `e2e.${payload}.signature`)
    localStorage.setItem(
      'af:auth:user',
      JSON.stringify({ id: 'user-1', email: 'dev@example.com', name: 'Dev', role: 'admin' })
    )
    localStorage.setItem('af:onboarding:completed', 'true')
    localStorage.setItem('af:nav:orgId', 'org-1')
    localStorage.setItem('af:nav:projectId', 'proj-1')
    localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
  })

  await page.route('**/api/v1/context/candidates**', async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    if (request.method() !== 'GET') return route.fallback()

    const state = url.searchParams.get('state') ?? 'pending'
    const itemKind = url.searchParams.get('item_kind') ?? 'all'
    const scopeKind = url.searchParams.get('scope_kind') ?? 'all'
    const data = candidates.filter((candidate) => {
      if (state !== 'all' && candidate.state !== state) return false
      if (itemKind !== 'all' && candidate.item_kind !== itemKind) return false
      if (scopeKind !== 'all' && candidate.proposed_scope_kind !== scopeKind) return false
      return true
    })

    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ ok: true, data }),
    })
  })

  await page.goto(`${baseURL}/context`)
  await page.locator('[data-testid="context-approval-page"]').waitFor({ state: 'visible' })
}

function candidate(overrides: Partial<E2eCandidate> = {}): E2eCandidate {
  const now = new Date('2026-05-06T09:00:00.000Z').toISOString()
  return {
    id: 'candidate-approval',
    workspace_id: 'workspace-1',
    item_kind: 'memory',
    state: 'pending',
    owner_user_id: 'user-1',
    source_run_id: 'run-1',
    target_skill_id: null,
    proposed_scope_kind: 'project',
    source_available: true,
    proposed_preview: {
      title: 'Production validation memory',
      content_preview:
        'Run make prod-ext, then check API, orchestrator, NATS, Prometheus, and Grafana.',
      sensitivity: 'internal',
    },
    created_at: now,
    updated_at: now,
    ...overrides,
  }
}

test.describe('Approval queue', () => {
  test('shows websocket candidates and approves with governance fields', async ({
    page,
    baseURL,
  }) => {
    const candidates = [candidate()]
    let approveBody: Record<string, unknown> | null = null
    await page.route('**/api/v1/context/candidates/**/approve', async (route) => {
      approveBody = route.request().postDataJSON() as Record<string, unknown>
      candidates[0] = { ...candidates[0], state: 'approved' }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, data: { candidate: candidates[0], approval: null } }),
      })
    })

    await setupAndNavigate(page, baseURL!, candidates)
    await expect(page.getByText('Production validation memory')).toBeVisible()

    candidates.push(
      candidate({
        id: 'candidate-realtime',
        item_kind: 'skill',
        proposed_scope_kind: 'team',
        proposed_preview: {
          title: 'Realtime skill candidate',
          content_preview: 'A completed task proposed a reusable workflow.',
        },
      })
    )
    await page.evaluate(() => {
      ;(window as unknown as { __afEmitWs: (payload: unknown) => void }).__afEmitWs({
        type: 'context_candidate.created',
        candidateId: 'candidate-realtime',
      })
    })
    await expect(page.getByText('Realtime skill candidate')).toBeVisible()

    await page.getByTestId('context-approve-candidate-approval').dispatchEvent('click')
    const dialog = page.getByRole('dialog', { name: /approve production validation memory/i })
    await expect(dialog).toBeVisible()
    await dialog.getByTestId('context-approval-scope-kind').selectOption('team')
    await dialog.getByTestId('context-approval-scope-id').fill('team-1')
    await dialog.getByLabel('TTL').fill('2030-01-01T12:00')
    await dialog.getByLabel('Sensitivity').selectOption('confidential')
    await dialog.getByLabel('Note').fill('Approved from queue')
    await dialog.getByRole('checkbox', { name: 'Confirm team scope expansion' }).check()
    await dialog.getByTestId('context-approval-submit').click()

    await expect(dialog).toBeHidden()
    expect(approveBody).toMatchObject({
      scope_kind: 'team',
      scope_id: 'team-1',
      sensitivity: 'confidential',
      reason: 'Approved from queue',
      confirm_expansion: true,
    })
    expect(String(approveBody?.ttl_at)).toContain('2030-01-01')
    await expect(page.getByText('Production validation memory')).toBeHidden()
  })

  test('rejects a candidate with a reason and removes it from pending', async ({
    page,
    baseURL,
  }) => {
    const candidates = [
      candidate({
        id: 'candidate-reject',
        proposed_preview: { title: 'Broad memory candidate', content_preview: 'Too generic.' },
      }),
    ]
    let rejectBody: Record<string, unknown> | null = null
    await page.route('**/api/v1/context/candidates/**/reject', async (route) => {
      rejectBody = route.request().postDataJSON() as Record<string, unknown>
      candidates[0] = { ...candidates[0], state: 'rejected' }
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true, data: { candidate: candidates[0], approval: null } }),
      })
    })

    await setupAndNavigate(page, baseURL!, candidates)
    await page.getByTestId('context-reject-candidate-reject').dispatchEvent('click')
    const dialog = page.getByRole('dialog', { name: /reject broad memory candidate/i })
    await dialog.getByTestId('context-reject-reason').fill('Too broad for governed memory')
    await dialog.getByTestId('context-reject-submit').click()

    await expect(dialog).toBeHidden()
    expect(rejectBody).toEqual({ reason: 'Too broad for governed memory' })
    await expect(page.getByText('Broad memory candidate')).toBeHidden()
  })

  test('disables approval when the source run is unavailable', async ({ page, baseURL }) => {
    const candidates = [
      candidate({
        id: 'candidate-unavailable',
        source_available: false,
        proposed_preview: { title: 'Unavailable source memory', content_preview: 'Missing run.' },
      }),
    ]

    await setupAndNavigate(page, baseURL!, candidates)

    await expect(page.getByText('Unavailable source memory')).toBeVisible()
    await expect(page.getByTestId('context-source-unavailable-candidate-unavailable')).toBeVisible()
    await expect(page.getByTestId('context-approve-candidate-unavailable')).toBeDisabled()
  })

  test('is operable on a 375px viewport', async ({ page, baseURL }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    const candidates = [candidate()]

    await setupAndNavigate(page, baseURL!, candidates)
    await page.getByTestId('context-approve-candidate-approval').dispatchEvent('click')

    const dialog = page.getByRole('dialog', { name: /approve production validation memory/i })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByTestId('context-approval-scope-kind')).toBeVisible()
    await expect(dialog.getByTestId('context-approval-submit')).toBeVisible()
  })
})
