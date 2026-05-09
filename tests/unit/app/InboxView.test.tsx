import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, waitFor } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { InboxView } from '@app/features/inbox/InboxView'
import { useFeedStore } from '@app/shared/model/feed.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useSettingsStore } from '@app/shared/model/settings.store'

const { navigateMock, orchestrationApiMock } = vi.hoisted(() => ({
  navigateMock: vi.fn(),
  orchestrationApiMock: {
    fetchInboxNotifications: vi.fn(),
    markInboxNotificationRead: vi.fn(),
    markAllInboxNotificationsRead: vi.fn(),
  },
}))

vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigateMock,
}))

vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: orchestrationApiMock,
}))

afterEach(cleanup)
beforeEach(() => {
  useFeedStore.getState().reset()
  useBoardStore.getState().reset()
  useSettingsStore.getState().setActiveSection('providers')
  navigateMock.mockClear()
  orchestrationApiMock.fetchInboxNotifications.mockResolvedValue([])
  orchestrationApiMock.markInboxNotificationRead.mockResolvedValue({ ok: true })
  orchestrationApiMock.markAllInboxNotificationsRead.mockResolvedValue({ ok: true })
})

describe('InboxView', () => {
  test('shows empty state when no notifications', () => {
    render(<InboxView />)
    expect(screen.getByText(/all caught up/i)).toBeDefined()
  })

  test('renders notification items', () => {
    useFeedStore.getState().addNotification({
      id: 'n1',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Deploy staging',
      message: 'Agent needs SSH key approval',
      read: false,
      timestamp: Date.now(),
    })
    render(<InboxView />)
    expect(screen.getByText('Deploy staging')).toBeDefined()
    const item = screen.getByTestId('inbox-notification-n1')
    expect(item.getAttribute('data-template')).toBe('task-lifecycle')
    expect(screen.getByText('Blocked task')).toBeDefined()
    expect(screen.getByText('Review blocker')).toBeDefined()
  })

  test('loads persisted failed owner notifications', async () => {
    orchestrationApiMock.fetchInboxNotifications.mockResolvedValue([
      {
        id: 'task-owner:t-failed:failed',
        type: 'failed',
        taskId: 't-failed',
        taskTitle: 'Deploy production',
        message: 'codex failed to complete this task: exit 1',
        taskHref: '/tasks',
        ownerUserId: 'owner-1',
        read: false,
        timestamp: Date.now(),
      },
    ])

    render(<InboxView />)

    const item = await screen.findByTestId('inbox-notification-task-owner:t-failed:failed')
    expect(item.getAttribute('data-template')).toBe('task-lifecycle')
    expect(screen.getByText('Failed task')).toBeDefined()
    expect(screen.getByText('View failure')).toBeDefined()
    expect(screen.getByText('Deploy production')).toBeDefined()
  })

  test('shows unread count', () => {
    const store = useFeedStore.getState()
    store.addNotification({
      id: 'n1',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Task A',
      message: 'Blocked',
      read: false,
      timestamp: Date.now(),
    })
    store.addNotification({
      id: 'n2',
      type: 'completed',
      taskId: 't2',
      taskTitle: 'Task B',
      message: 'Done',
      read: true,
      timestamp: Date.now(),
    })
    render(<InboxView />)
    // Badge now reads "N new" instead of just the number
    expect(screen.getByTestId('unread-count').textContent).toMatch(/^1\s*new$/)
  })

  test('opens linked task notifications and marks them read', async () => {
    useFeedStore.getState().addNotification({
      id: 'task-owner:t1:blocked',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Task A',
      message: 'Blocked',
      taskHref: '/tasks',
      read: false,
      timestamp: Date.now(),
    })

    render(<InboxView />)
    await userEvent.setup().click(screen.getByTestId('inbox-notification-task-owner:t1:blocked'))

    expect(useFeedStore.getState().notifications[0].read).toBe(true)
    await waitFor(() =>
      expect(orchestrationApiMock.markInboxNotificationRead).toHaveBeenCalledWith(
        'task-owner:t1:blocked'
      )
    )
    expect(useBoardStore.getState().selectedTaskId).toBe('t1')
    expect(navigateMock).toHaveBeenCalledWith({ to: '/tasks' })
  })

  test('renders credential notifications with action styling and opens settings', async () => {
    useFeedStore.getState().addNotification({
      id: 'credential-owner:user-owner:codex:expired:evt-1',
      type: 'credential_expired',
      taskId: 'credential:codex',
      taskTitle: 'Codex credential expired',
      message: 'Codex Container CLI credential expired',
      taskHref: '/settings',
      read: false,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    const item = screen.getByTestId(
      'inbox-notification-credential-owner:user-owner:codex:expired:evt-1'
    )
    expect(item.getAttribute('data-template')).toBe('credential-action')
    expect(item.className).toContain('bg-apple-blue/[0.04]')
    expect(screen.getByText('Credential')).toBeDefined()
    expect(screen.getByText('Reconnect credential')).toBeDefined()

    await userEvent.setup().click(item)

    expect(useFeedStore.getState().notifications[0].read).toBe(true)
    expect(useSettingsStore.getState().activeSection).toBe('runtime')
    expect(navigateMock).toHaveBeenCalledWith({
      to: '/settings/$section',
      params: { section: 'runtime' },
    })
  })
})
