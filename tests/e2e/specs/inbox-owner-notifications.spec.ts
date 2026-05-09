import type { Page } from '@playwright/test'
import { test, expect } from '../fixtures/app-fixtures'

function encodeBase64Url(value: unknown): string {
  return Buffer.from(JSON.stringify(value), 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
}

async function injectOwnerAuth(page: Page, userId = 'user-e2e'): Promise<void> {
  const exp = Math.floor(Date.now() / 1000) + 3600
  const token = `${encodeBase64Url({ alg: 'none', typ: 'JWT' })}.${encodeBase64Url({
    sub: userId,
    exp,
    orgId: 'org-1',
    role: 'admin',
  })}.signature`

  await page.addInitScript(
    ({ accessToken, ownerId }) => {
      localStorage.setItem('af:auth:access', accessToken)
      localStorage.setItem(
        'af:auth:user',
        JSON.stringify({
          id: ownerId,
          email: 'owner@example.test',
          username: 'owner',
          orgId: 'org-1',
          role: 'admin',
        })
      )
      localStorage.setItem('af:onboarding:completed', 'true')
      localStorage.setItem('af:nav:orgId', 'org-1')
      localStorage.setItem('af:nav:projectId', 'proj-1')
      localStorage.setItem('af:nav:expandedTeams', '["team-1"]')
    },
    { accessToken: token, ownerId: userId }
  )
}

interface OwnerTaskUpdate {
  id: string
  title: string
  state: 'blocked' | 'completed'
  priority: 'normal' | 'high'
  assignedAgentName: string
  ownerId?: string
  blockedHint?: string
  result?: { message: string }
}

async function installOwnerTaskWebSocket(page: Page, taskUpdate: OwnerTaskUpdate): Promise<void> {
  await page.addInitScript(
    ({ update }) => {
      class MockWebSocket {
        static CONNECTING = 0
        static OPEN = 1
        static CLOSING = 2
        static CLOSED = 3

        readyState = MockWebSocket.CONNECTING
        onopen: ((event: Event) => void) | null = null
        onclose: ((event: Event) => void) | null = null
        onerror: ((event: Event) => void) | null = null
        onmessage: ((event: MessageEvent) => void) | null = null

        constructor() {
          window.setTimeout(() => {
            this.readyState = MockWebSocket.OPEN
            this.onopen?.(new Event('open'))
            this.emitOwnerTaskUpdate()
          }, 25)
        }

        send(raw: string): void {
          try {
            const message = JSON.parse(raw) as { type?: string }
            if (message.type !== 'auth') return
          } catch {
            return
          }

          this.emitOwnerTaskUpdate()
        }

        emitOwnerTaskUpdate(): void {
          window.setTimeout(() => {
            const now = new Date().toISOString()
            const blockedHint = update.state === 'blocked' ? update.blockedHint : undefined
            this.onmessage?.(
              new MessageEvent('message', {
                data: JSON.stringify({
                  type: 'orchestration:task_update',
                  payload: {
                    task: {
                      id: update.id,
                      groupId: 'grp-1',
                      state: update.state,
                      method: 'agents/run',
                      params: { task: update.title, message: '' },
                      assignedTo: `agent-${update.id}`,
                      assignedAgentName: update.assignedAgentName,
                      priority: update.priority,
                      progress: update.state === 'completed' ? 100 : 0,
                      createdBy: update.ownerId ?? 'user-e2e',
                      createdAt: now,
                      updatedAt: now,
                      blockedHint,
                      result: update.result,
                    },
                  },
                }),
              })
            )
          }, 250)
        }

        close(): void {
          this.readyState = MockWebSocket.CLOSED
          this.onclose?.(new Event('close'))
        }
      }

      window.WebSocket = MockWebSocket as unknown as typeof WebSocket
    },
    { update: taskUpdate }
  )
}

async function openInboxWithOwnerUpdate(page: Page, baseURL: string, update: OwnerTaskUpdate) {
  await installOwnerTaskWebSocket(page, update)
  await injectOwnerAuth(page)
  await page.goto(`${baseURL}/inbox`)
  await page.locator('[data-testid="top-bar"]').waitFor({ state: 'visible', timeout: 15_000 })
}

test.describe('Inbox owner task notifications', () => {
  test('blocked owner task update creates a linked Inbox notification', async ({
    page,
    baseURL,
  }) => {
    await openInboxWithOwnerUpdate(page, baseURL!, {
      id: 't-004',
      title: 'Review PR #42',
      state: 'blocked',
      priority: 'high',
      assignedAgentName: 'Claude',
      blockedHint: 'Waiting for owner approval',
    })

    const notification = page.locator('[data-testid="inbox-notification-task-owner:t-004:blocked"]')
    await expect(notification).toBeVisible({ timeout: 10_000 })
    await expect(notification).toContainText('Review PR #42')
    await expect(notification).toContainText('Waiting for owner approval')

    await notification.click()
    await page.waitForURL('**/tasks')
    await expect(page.locator('[data-testid="task-card-t-004"]')).toBeAttached({
      timeout: 10_000,
    })
    await expect(page.locator('[data-testid="right-panel"]')).toContainText('Review PR #42', {
      timeout: 5000,
    })
  })

  test('completed owner task update creates a linked Inbox notification', async ({
    page,
    baseURL,
  }) => {
    await openInboxWithOwnerUpdate(page, baseURL!, {
      id: 't-005',
      title: 'Deploy v2.1.0 to staging',
      state: 'completed',
      priority: 'normal',
      assignedAgentName: 'Claude',
      result: { message: 'Released to staging' },
    })

    const notification = page.locator(
      '[data-testid="inbox-notification-task-owner:t-005:completed"]'
    )
    await expect(notification).toBeVisible({ timeout: 10_000 })
    await expect(notification).toContainText('Deploy v2.1.0 to staging')
    await expect(notification).toContainText('Released to staging')

    await notification.click()
    await page.waitForURL('**/tasks')
    await expect(page.locator('[data-testid="task-card-t-005"]')).toBeAttached({
      timeout: 10_000,
    })
    await expect(page.locator('[data-testid="right-panel"]')).toContainText(
      'Deploy v2.1.0 to staging',
      { timeout: 5000 }
    )
  })
})
