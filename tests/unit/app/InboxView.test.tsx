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
    expect(screen.getByText('Inbox triage path')).toBeDefined()
    expect(screen.getByText(/start with needs action/i)).toBeDefined()
    expect(
      screen.getByText(/use credentials when an agent needs access reconnected/i)
    ).toBeDefined()
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
    expect(screen.getByText(/provide the requested input/i)).toBeDefined()
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

  test('summarizes the safest next action for beginner triage', () => {
    const store = useFeedStore.getState()
    store.addNotification({
      id: 'n1',
      type: 'completed',
      taskId: 't1',
      taskTitle: 'Generate report',
      message: 'Ready for review',
      read: false,
      timestamp: Date.now() - 1000,
    })
    store.addNotification({
      id: 'n2',
      type: 'blocked',
      taskId: 't2',
      taskTitle: 'Deploy staging',
      message: 'Waiting for SSH access',
      read: false,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    const nextStep = screen.getByTestId('inbox-next-step')
    expect(nextStep).toHaveTextContent('Do This Next')
    expect(nextStep).toHaveTextContent('Review the blocker that is stopping work')
    expect(nextStep).toHaveTextContent('This is the only item that needs action')
    expect(screen.getByRole('button', { name: /open blocked task/i })).toBeDefined()
  })

  test('prioritizes expired credentials because they can block future runs', () => {
    const store = useFeedStore.getState()
    store.addNotification({
      id: 'n1',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Blocked deployment',
      message: 'Waiting for input',
      read: false,
      timestamp: Date.now(),
    })
    store.addNotification({
      id: 'n2',
      type: 'credential_expired',
      taskId: 'credential:codex',
      taskTitle: 'Credential expired',
      message: 'Reconnect runtime access',
      taskHref: '/settings',
      read: false,
      timestamp: Date.now() - 1000,
    })

    render(<InboxView />)

    const nextStep = screen.getByTestId('inbox-next-step')
    expect(nextStep).toHaveTextContent('Reconnect a credential before more agent work starts')
    expect(nextStep).toHaveTextContent('keeps future agent runs from failing')
    expect(screen.getByRole('button', { name: /open settings/i })).toBeDefined()
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
    expect(screen.getByRole('button', { name: /mark all as read/i })).toBeDefined()
  })

  test('filters notifications by triage lane', async () => {
    const store = useFeedStore.getState()
    store.addNotification({
      id: 'n1',
      type: 'blocked',
      taskId: 't1',
      taskTitle: 'Blocked deployment',
      message: 'Needs owner input',
      read: false,
      timestamp: Date.now() - 1000,
    })
    store.addNotification({
      id: 'n2',
      type: 'completed',
      taskId: 't2',
      taskTitle: 'Completed cleanup',
      message: 'Ready for review',
      read: true,
      timestamp: Date.now() - 2000,
    })
    store.addNotification({
      id: 'n3',
      type: 'credential_expired',
      taskId: 'credential:codex',
      taskTitle: 'Credential expired',
      message: 'Reconnect runtime access',
      taskHref: '/settings',
      read: false,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    expect(screen.getByTestId('inbox-filter-count-all').textContent).toBe('3')
    expect(screen.getByTestId('inbox-filter-count-unread').textContent).toBe('2')
    expect(screen.getByTestId('inbox-filter-count-needs-action').textContent).toBe('2')
    expect(screen.getByTestId('inbox-filter-count-credentials').textContent).toBe('1')

    const user = userEvent.setup()
    await user.click(screen.getByTestId('inbox-filter-needs-action'))
    expect(screen.getByText('Blocked deployment')).toBeDefined()
    expect(screen.getByText('Credential expired')).toBeDefined()
    expect(screen.queryByText('Completed cleanup')).toBeNull()

    await user.click(screen.getByTestId('inbox-filter-credentials'))
    expect(screen.getByText('Credential expired')).toBeDefined()
    expect(screen.queryByText('Blocked deployment')).toBeNull()

    await user.click(screen.getByTestId('inbox-filter-unread'))
    expect(screen.getByText('Blocked deployment')).toBeDefined()
    expect(screen.getByText('Credential expired')).toBeDefined()
    expect(screen.queryByText('Completed cleanup')).toBeNull()

    await user.click(screen.getByTestId('inbox-filter-credentials'))
    expect(screen.getByText('Credential expired')).toBeDefined()
  })

  test('explains what to try when a filter has no notifications', async () => {
    useFeedStore.getState().addNotification({
      id: 'n-completed',
      type: 'completed',
      taskId: 't-done',
      taskTitle: 'Completed cleanup',
      message: 'Ready for review',
      read: true,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    await userEvent.setup().click(screen.getByTestId('inbox-filter-credentials'))

    expect(screen.getByTestId('inbox-filter-empty')).toBeDefined()
    expect(screen.getByText(/try all for the full history/i)).toBeDefined()
    expect(screen.getByText(/needs action for items that still need a response/i)).toBeDefined()
  })

  test('explains an empty filtered lane and lets the user return to all updates', async () => {
    useFeedStore.getState().addNotification({
      id: 'n1',
      type: 'completed',
      taskId: 't1',
      taskTitle: 'Completed cleanup',
      message: 'Ready for review',
      read: true,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    const user = userEvent.setup()
    await user.click(screen.getByTestId('inbox-filter-credentials'))

    expect(screen.getByTestId('inbox-filter-empty')).toHaveTextContent(
      'No credentials need reconnecting right now.'
    )

    await user.click(screen.getByRole('button', { name: /show all notifications/i }))

    expect(screen.getByText('Completed cleanup')).toBeDefined()
  })

  test('shows a recoverable message when older notifications cannot load', async () => {
    orchestrationApiMock.fetchInboxNotifications.mockRejectedValue(new Error('offline'))
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    render(<InboxView />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Saved notifications could not be loaded')
    expect(alert).toHaveTextContent('Check your connection, then reload the inbox.')

    await userEvent.setup().click(screen.getByRole('button', { name: /reload inbox/i }))
    await waitFor(() =>
      expect(orchestrationApiMock.fetchInboxNotifications).toHaveBeenCalledTimes(2)
    )

    warnSpy.mockRestore()
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
      message: 'Codex work-tool credential expired',
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
