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

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

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
  test('shows empty state when no notifications', async () => {
    render(<InboxView />)
    expect(await screen.findByText(/all caught up/i)).toBeDefined()
    expect(screen.getByText('Inbox triage path')).toBeDefined()
    expect(screen.getByText(/start with needs action/i)).toBeDefined()
    expect(screen.getByText(/tasks that need help/i)).toBeDefined()
    expect(screen.getByText(/stopped early/i)).toBeDefined()
    expect(screen.getByText(/account access notices/i)).toBeDefined()
    expect(
      screen.getByText(/use account access when an agent needs you to reconnect a work account/i)
    ).toBeDefined()
    expect(screen.queryByText(/needs a connection restored/i)).toBeNull()
    expect(screen.queryByText(/failures/i)).toBeNull()
    expect(screen.queryByText(/system alerts/i)).toBeNull()
  })

  test('shows progress while older saved updates are loading', async () => {
    const request = deferred<never[]>()
    orchestrationApiMock.fetchInboxNotifications.mockReturnValueOnce(request.promise)

    render(<InboxView />)

    expect(screen.getByRole('status')).toHaveTextContent('Checking for saved updates...')
    expect(screen.getByText('Checking for saved updates')).toBeDefined()
    expect(screen.getByText(/older notifications/i)).toBeDefined()

    request.resolve([])
    expect(await screen.findByText(/all caught up/i)).toBeDefined()
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
    expect(screen.getByText('Needs help')).toBeDefined()
    expect(screen.getByText('Review what needs help')).toBeDefined()
    expect(screen.getByText(/provide the requested input/i)).toBeDefined()
    expect(screen.queryByText(/review blocker/i)).toBeNull()
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
    expect(screen.getByText('Recovery needed')).toBeDefined()
    expect(screen.getAllByText('Review recovery').length).toBeGreaterThan(0)
    expect(screen.queryByText('Failed task')).toBeNull()
    expect(screen.queryByText('View failure')).toBeNull()
    expect(screen.getByTestId('inbox-next-step')).toHaveTextContent(
      'Review the recovery note before retrying'
    )
    expect(screen.getByRole('button', { name: /^review recovery$/i })).toBeDefined()
    expect(screen.queryByText('Open Failed Task')).toBeNull()
    expect(screen.getByText('Deploy production')).toBeDefined()
    expect(
      screen.getByText(
        'The task stopped before finishing. Open it, review the recovery note, then retry or choose another agent.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/when ready/i)).toBeNull()
    expect(screen.queryByText(/reassign/i)).toBeNull()
    expect(screen.queryByText(/failed to complete this task/i)).toBeNull()
    expect(screen.queryByText(/exit 1/i)).toBeNull()
  })

  test('hides raw details from older failed notifications', async () => {
    orchestrationApiMock.fetchInboxNotifications.mockResolvedValue([
      {
        id: 'task-owner:t-raw-failed:failed',
        type: 'failed',
        taskId: 't-raw-failed',
        taskTitle: 'Refresh code access',
        message: 'git provider token rejected: HTTP 401 Unauthorized',
        taskHref: '/tasks',
        ownerUserId: 'owner-1',
        read: false,
        timestamp: Date.now(),
      },
    ])

    render(<InboxView />)

    await screen.findByTestId('inbox-notification-task-owner:t-raw-failed:failed')
    expect(
      screen.getByText(
        'The task stopped before finishing. Open it, review the recovery note, then retry or choose another agent.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/provider token/i)).toBeNull()
    expect(screen.queryByText(/HTTP 401/i)).toBeNull()
    expect(screen.queryByText(/Unauthorized/i)).toBeNull()
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
    expect(nextStep).toHaveTextContent('Do this next')
    expect(nextStep).toHaveTextContent('Review what is stopping work')
    expect(nextStep).toHaveTextContent('This is the only item that needs action')
    expect(screen.getByRole('button', { name: /^open task$/i })).toBeDefined()
    expect(nextStep).not.toHaveTextContent(/blocker/i)
    expect(nextStep).not.toHaveTextContent('Do This Next')
  })

  test('keeps multiple action guidance direct and avoids recovery-item phrasing', () => {
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
      type: 'failed',
      taskId: 't2',
      taskTitle: 'Stopped cleanup',
      message: 'The task stopped before finishing.',
      read: false,
      timestamp: Date.now() - 1000,
    })

    render(<InboxView />)

    const nextStep = screen.getByTestId('inbox-next-step')
    expect(nextStep).toHaveTextContent('2 items need action')
    expect(nextStep).toHaveTextContent('Start with the newest item that needs help.')
    expect(nextStep).not.toHaveTextContent(/recovery item/i)
  })

  test('prioritizes expired account access because it can block future work', () => {
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
    expect(nextStep).toHaveTextContent('Reconnect account access before more agent work starts')
    expect(nextStep).toHaveTextContent('keeps future agent work from failing')
    expect(nextStep).not.toHaveTextContent(/agent runs/i)
    expect(screen.getByText('Reconnect agent work access')).toBeDefined()
    expect(screen.getByText('Account access needs reconnecting')).toBeDefined()
    expect(screen.queryByText(/runtime access/i)).toBeNull()
    expect(screen.queryByText(/credential expired/i)).toBeNull()
    expect(screen.getByRole('button', { name: /open agent work settings/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: /^open settings$/i })).toBeNull()
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
    expect(screen.getByText('Account access needs reconnecting')).toBeDefined()
    expect(screen.queryByText('Completed cleanup')).toBeNull()

    await user.click(screen.getByTestId('inbox-filter-credentials'))
    expect(screen.getByText('Account access needs reconnecting')).toBeDefined()
    expect(screen.queryByText('Blocked deployment')).toBeNull()

    await user.click(screen.getByTestId('inbox-filter-unread'))
    expect(screen.getByText('Blocked deployment')).toBeDefined()
    expect(screen.getByText('Account access needs reconnecting')).toBeDefined()
    expect(screen.queryByText('Completed cleanup')).toBeNull()

    await user.click(screen.getByTestId('inbox-filter-credentials'))
    expect(screen.getByText('Account access needs reconnecting')).toBeDefined()
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
    expect(screen.getByText('No account access needs reconnecting')).toBeDefined()
    expect(screen.getByText(/open all to review other updates/i)).toBeDefined()
    expect(screen.queryByText(/try all for the full history/i)).toBeNull()
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

    const emptyState = screen.getByTestId('inbox-filter-empty')
    expect(emptyState).toHaveTextContent('No account access needs reconnecting')
    expect(emptyState).toHaveTextContent('Account access is not blocking agent work right now.')
    expect(emptyState).not.toHaveTextContent('No account access needs reconnecting right now.')

    await user.click(screen.getByRole('button', { name: /show all updates/i }))

    expect(screen.getByText('Completed cleanup')).toBeDefined()
  })

  test('explains empty needs-action lane without failure jargon', async () => {
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

    await userEvent.setup().click(screen.getByTestId('inbox-filter-needs-action'))

    const emptyState = screen.getByTestId('inbox-filter-empty')
    expect(emptyState).toHaveTextContent('You are caught up on action items')
    expect(emptyState).toHaveTextContent(
      'No task is asking for help and no account access needs reconnecting.'
    )
    expect(emptyState).not.toHaveTextContent('Nothing needs action right now')
    expect(emptyState).not.toHaveTextContent(
      'No tasks that need help, stopped work, or account access issues need action right now.'
    )
    expect(emptyState).not.toHaveTextContent(/blockers/i)
    expect(emptyState).not.toHaveTextContent(/failures/i)
  })

  test('guides unread empty state back to older updates', async () => {
    useFeedStore.getState().addNotification({
      id: 'n-read',
      type: 'completed',
      taskId: 't-done',
      taskTitle: 'Completed cleanup',
      message: 'Ready for review',
      read: true,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    await userEvent.setup().click(screen.getByTestId('inbox-filter-unread'))

    const emptyState = screen.getByTestId('inbox-filter-empty')
    expect(emptyState).toHaveTextContent('No unread updates')
    expect(emptyState).toHaveTextContent(
      'Older updates are still in All. Open All if you need the full history.'
    )
    expect(emptyState).not.toHaveTextContent('Nothing new is waiting for you.')
  })

  test('shows a recoverable message when older notifications cannot load', async () => {
    const retry = deferred<never[]>()
    orchestrationApiMock.fetchInboxNotifications
      .mockRejectedValueOnce(new Error('offline'))
      .mockReturnValueOnce(retry.promise)
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    render(<InboxView />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Check your connection, then reload the inbox. Saved notifications could not be loaded'
    )
    expect(alert.textContent).not.toMatch(/^Saved notifications could not be loaded/)

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /reload inbox/i }))
    expect(screen.getByRole('button', { name: /reloading inbox/i })).toBeDisabled()
    expect(orchestrationApiMock.fetchInboxNotifications).toHaveBeenCalledTimes(2)

    retry.resolve([])
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
    expect(screen.getByText(/all caught up/i)).toBeDefined()

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

  test('renders account access notifications with action styling and opens settings', async () => {
    useFeedStore.getState().addNotification({
      id: 'credential-owner:user-owner:codex:expired:evt-1',
      type: 'credential_expired',
      taskId: 'credential:codex',
      taskTitle: 'Codex credential expired',
      message: 'Codex account connection expired',
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
    expect(item).toHaveTextContent('Account access')
    expect(item).toHaveTextContent('Reconnect work access')
    expect(item).toHaveTextContent(
      'Open agent work settings and reconnect the account agents use for file work.'
    )
    expect(screen.getByText('Codex account access needs reconnecting')).toBeDefined()
    expect(screen.getByText('Codex work account needs reconnecting')).toBeDefined()
    expect(screen.queryByText(/credential expired/i)).toBeNull()
    expect(screen.queryByText(/account connection expired/i)).toBeNull()
    expect(screen.queryByText('Codex account connection needs reconnecting')).toBeNull()

    await userEvent.setup().click(item)

    expect(useFeedStore.getState().notifications[0].read).toBe(true)
    expect(useSettingsStore.getState().activeSection).toBe('runtime')
    expect(navigateMock).toHaveBeenCalledWith({
      to: '/settings/$section',
      params: { section: 'runtime' },
    })
  })

  test('renders tool update notifications with the current admin label', () => {
    useFeedStore.getState().addNotification({
      id: 'cli-image:codex:updated',
      type: 'cli_image_updated',
      taskId: 'cli-image:codex',
      taskTitle: 'Codex tool package updated',
      message: 'New agents will use the latest package.',
      taskHref: '/admin?section=cli-images',
      read: false,
      timestamp: Date.now(),
    })

    render(<InboxView />)

    const item = screen.getByTestId('inbox-notification-cli-image:codex:updated')
    expect(item.getAttribute('data-template')).toBe('task-lifecycle')
    expect(screen.getByText('Tool update')).toBeDefined()
    expect(screen.getAllByText('Open tool updates').length).toBeGreaterThan(0)
    expect(screen.getByText(/open admin, then agent tool updates/i)).toBeDefined()
    expect(screen.getByText(/check each work tool/i)).toBeDefined()
    expect(screen.getByTestId('inbox-next-step')).toHaveTextContent(
      'Review the latest agent tool update'
    )
    expect(screen.getAllByRole('button', { name: /open tool updates/i }).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /open cli images/i })).toBeNull()
    expect(screen.queryByText(['Work', '-tool image'].join(''))).toBeNull()
    expect(screen.queryByText(['Open work', '-tool images'].join(''))).toBeNull()
    expect(screen.queryByText(/per-tool status/i)).toBeNull()
  })
})
