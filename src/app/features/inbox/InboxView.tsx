import { useCallback, useEffect, useMemo, useState } from 'react'
import { Inbox as InboxIcon, RefreshCw } from 'lucide-react'
import { useNavigate } from '@tanstack/react-router'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { useFeedStore, type Notification } from '@app/entities/feed'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { PreferenceGuideDisclosure, useSettingsStore } from '@app/entities/settings'
import { useAdminStore } from '@app/entities/admin'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { InboxItem } from './InboxItem'

type InboxFilter = 'all' | 'unread' | 'needs-action' | 'credentials'

interface InboxFilterEmptyState {
  title: string
  detail: string
}

const FILTERS: { id: InboxFilter; label: string; ariaLabel: string }[] = [
  { id: 'all', label: 'All', ariaLabel: 'Show all inbox updates' },
  { id: 'unread', label: 'Unread', ariaLabel: 'Show unread inbox updates' },
  {
    id: 'needs-action',
    label: 'Needs action',
    ariaLabel: 'Show inbox updates that need action',
  },
  {
    id: 'credentials',
    label: 'Account access',
    ariaLabel: 'Show account access updates',
  },
]

const INBOX_ACTION_STEPS = [
  'Start with Needs action to find tasks that need help or stopped early.',
  'Use Account access when an agent needs you to reconnect a work account.',
  'Mark items read after the task or setting has been handled.',
]

const READ_STATUS_SAVE_ERROR =
  'Check your connection, then open Inbox again. Some updates may appear unread again because Forge could not save the read status.'

function InboxLoadError({ loading, onRetry }: { loading: boolean; onRetry: () => void }) {
  return (
    <div
      role="alert"
      aria-live="polite"
      className={cn(
        uiStyles.error,
        'mb-0 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between'
      )}
    >
      <span>
        Check your connection, then choose Check updates again. Saved updates could not be loaded,
        but new live updates will still appear here.
      </span>
      <button type="button" onClick={onRetry} disabled={loading} className={uiStyles.dangerButton}>
        <RefreshCw size={14} className={cn(loading && 'animate-spin')} aria-hidden="true" />
        {loading ? 'Checking updates...' : 'Check updates again'}
      </button>
    </div>
  )
}

export function InboxView() {
  const { notifications, addNotification, markRead, markAllRead } = useFeedStore()
  const setSelectedTask = useBoardStore((s) => s.setSelectedTask)
  const navigate = useNavigate()
  const [activeFilter, setActiveFilter] = useState<InboxFilter>('all')
  const [loadError, setLoadError] = useState(false)
  const [readError, setReadError] = useState<string | null>(null)
  const [loadingSavedNotifications, setLoadingSavedNotifications] = useState(false)
  const unreadCount = notifications.filter((n) => !n.read).length
  const orderedNotifications = useMemo(
    () => [...notifications].sort((a, b) => b.timestamp - a.timestamp),
    [notifications]
  )
  const unreadNeedsActionCount = useMemo(
    () =>
      notifications.filter(
        (notification) => !notification.read && isActionNotification(notification)
      ).length,
    [notifications]
  )
  const unreadCredentialCount = useMemo(
    () =>
      notifications.filter(
        (notification) => !notification.read && notification.type === 'credential_expired'
      ).length,
    [notifications]
  )
  const filteredNotifications = useMemo(
    () => orderedNotifications.filter((notification) => matchesFilter(notification, activeFilter)),
    [activeFilter, orderedNotifications]
  )
  const filterEmptyState = inboxFilterEmptyState(activeFilter)
  const filterCounts = useMemo(
    () =>
      FILTERS.reduce(
        (acc, filter) => ({
          ...acc,
          [filter.id]: notifications.filter((notification) =>
            matchesFilter(notification, filter.id)
          ).length,
        }),
        {} as Record<InboxFilter, number>
      ),
    [notifications]
  )
  const nextStepNotification = useMemo(() => {
    const unreadNotifications = orderedNotifications.filter((notification) => !notification.read)
    return (
      unreadNotifications.find((notification) => notification.type === 'credential_expired') ??
      unreadNotifications.find((notification) => notification.type === 'blocked') ??
      unreadNotifications.find((notification) => notification.type === 'failed') ??
      // An overdue review needs a human decision before work continues, so it
      // must be surfaced as the next step ahead of any non-action update (a
      // newer completed/assigned item would otherwise win the newest-unread
      // fallback below).
      unreadNotifications.find((notification) => notification.type === 'review_escalated') ??
      unreadNotifications[0] ??
      orderedNotifications[0]
    )
  }, [orderedNotifications])
  const loadNotifications = useCallback(() => {
    let cancelled = false
    setLoadingSavedNotifications(true)
    orchestrationApi
      .fetchInboxNotifications()
      .then((items) => {
        if (cancelled) return
        setLoadError(false)
        items.forEach((item) => addNotification(item))
      })
      .catch((error) => {
        if (cancelled) return
        console.warn('Failed to load inbox notifications', error)
        setLoadError(true)
      })
      .finally(() => {
        if (cancelled) return
        setLoadingSavedNotifications(false)
      })
    return () => {
      cancelled = true
    }
  }, [addNotification])

  useEffect(() => loadNotifications(), [loadNotifications])

  function handleOpenNotification(notification: (typeof notifications)[number]) {
    setReadError(null)
    markRead(notification.id)
    void orchestrationApi.markInboxNotificationRead(notification.id).catch((error) => {
      console.warn('Failed to mark inbox notification read', error)
      setReadError(READ_STATUS_SAVE_ERROR)
    })
    if (notification.taskHref === '/tasks') {
      setSelectedTask(notification.taskId)
      void navigate({ to: '/tasks' })
    } else if (
      notification.type === 'credential_expired' ||
      notification.taskHref === '/settings/work-tool-sign-ins'
    ) {
      useSettingsStore.getState().setActiveSection('work-tool-sign-ins')
      void navigate({ to: '/settings/$section', params: { section: 'work-tool-sign-ins' } })
    } else if (notification.taskHref === '/settings') {
      useSettingsStore.getState().setActiveSection('runtime')
      void navigate({ to: '/settings/$section', params: { section: 'runtime' } })
    } else if (notification.taskHref === '/admin') {
      // Tool update notifications open the admin console on the tool updates panel,
      // mirroring the /settings runtime-section pattern above.
      useAdminStore.getState().setActiveSection('cli-images')
      void navigate({ to: '/admin' })
    }
  }

  function handleMarkAllRead() {
    setReadError(null)
    markAllRead()
    void orchestrationApi.markAllInboxNotificationsRead().catch((error) => {
      console.warn('Failed to mark inbox notifications read', error)
      setReadError(READ_STATUS_SAVE_ERROR)
    })
  }

  if (notifications.length === 0) {
    const checkingSavedUpdates = loadingSavedNotifications && !loadError
    return (
      <div className="mx-auto flex h-full w-full max-w-sm flex-col items-center justify-center gap-3 px-6 text-center">
        <div className="flex h-9 w-9 items-center justify-center rounded-card bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
          <InboxIcon size={20} strokeWidth={1.9} aria-hidden="true" />
        </div>
        {loadError && (
          <div className="w-full">
            <InboxLoadError loading={loadingSavedNotifications} onRetry={loadNotifications} />
          </div>
        )}
        {checkingSavedUpdates && (
          <div role="status" aria-live="polite" className={cn(uiStyles.badge, 'gap-2')}>
            <RefreshCw size={13} className="animate-spin" aria-hidden="true" />
            Checking for saved updates...
          </div>
        )}
        <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          {checkingSavedUpdates ? 'Checking for saved updates' : 'No updates yet'}
        </p>
        {!loadError && <InboxActionPath compact checkingSavedUpdates={checkingSavedUpdates} />}
        {!loadError && !checkingSavedUpdates && (
          <button type="button" onClick={loadNotifications} className={uiStyles.secondaryButton}>
            <RefreshCw size={14} aria-hidden="true" />
            Check updates again
          </button>
        )}
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06]">
        <header className="mb-3">
          <h1 className="text-ui-title font-semibold text-foreground-light dark:text-foreground-dark">
            Inbox
          </h1>
          <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
            Start with tasks that need help and account access issues. Completed work can wait until
            you have time to check it.
          </p>
        </header>
        {nextStepNotification && (
          <div data-testid="inbox-next-step" className="mb-3">
            <PreferenceGuideDisclosure
              guideKey="inbox-next-step"
              icon={<InboxIcon />}
              title="Do this next"
            >
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0">
                  <p className="mt-0.5 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                    {nextStepTitle(nextStepNotification)}
                  </p>
                  <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {nextStepDescription(
                      nextStepNotification,
                      unreadNeedsActionCount,
                      unreadCredentialCount
                    )}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => handleOpenNotification(nextStepNotification)}
                  className={uiStyles.primaryButton}
                >
                  {nextStepActionLabel(nextStepNotification)}
                </button>
              </div>
            </PreferenceGuideDisclosure>
          </div>
        )}
        {loadError && (
          <div className="mb-3">
            <InboxLoadError loading={loadingSavedNotifications} onRetry={loadNotifications} />
          </div>
        )}
        {readError && (
          <div role="alert" aria-live="polite" className={cn(uiStyles.note, 'mb-3')}>
            {readError}
          </div>
        )}
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {filteredNotifications.length} of {notifications.length}{' '}
              {notifications.length === 1 ? 'update' : 'updates'}
            </p>
            {loadingSavedNotifications && !loadError && (
              <span role="status" aria-live="polite" className={cn(uiStyles.badge, 'gap-1.5')}>
                <RefreshCw size={12} className="animate-spin" aria-hidden="true" />
                Checking older saved updates...
              </span>
            )}
            {unreadCount > 0 && (
              <span
                data-testid="unread-count"
                className="inline-flex items-center gap-1.5 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
              >
                <span className="h-1.5 w-1.5 rounded-full bg-apple-blue" />
                {unreadCount} new
              </span>
            )}
          </div>
          {unreadCount > 0 && (
            <button type="button" onClick={handleMarkAllRead} className={uiStyles.subtleButton}>
              Mark all as read
            </button>
          )}
        </div>
        <div
          className="mt-3 flex flex-wrap gap-1 rounded-button bg-black/[0.035] p-1 dark:bg-white/[0.05]"
          role="group"
          aria-label="Inbox filters"
        >
          {FILTERS.map((filter) => {
            const selected = filter.id === activeFilter
            return (
              <button
                key={filter.id}
                type="button"
                data-testid={`inbox-filter-${filter.id}`}
                aria-pressed={selected}
                aria-label={`${filter.ariaLabel}, ${matchingUpdatesLabel(filterCounts[filter.id])}`}
                onClick={() => setActiveFilter(filter.id)}
                className={cn(
                  'flex h-8 items-center gap-1.5 rounded-button px-2.5 text-ui-button font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                  selected
                    ? 'bg-black/[0.06] text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark'
                    : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
                )}
              >
                <span>{filter.label}</span>
                <span
                  data-testid={`inbox-filter-count-${filter.id}`}
                  aria-hidden="true"
                  className="text-ui-caption font-medium tabular-nums text-secondary-light dark:text-secondary-dark"
                >
                  {filterCounts[filter.id]}
                </span>
              </button>
            )
          })}
        </div>
        <InboxActionPath />
      </div>
      <div className="flex-1 divide-y divide-black/[0.04] overflow-y-auto dark:divide-white/[0.04]">
        {filteredNotifications.length > 0 ? (
          filteredNotifications.map((n) => (
            <InboxItem key={n.id} notification={n} onClick={() => handleOpenNotification(n)} />
          ))
        ) : (
          <div
            data-testid="inbox-filter-empty"
            role="status"
            aria-live="polite"
            className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark"
          >
            <InboxIcon size={20} strokeWidth={1.9} aria-hidden="true" />
            <PreferenceGuideDisclosure
              guideKey="inbox-filter-empty-help"
              icon={<InboxIcon />}
              title={filterEmptyState.title}
              className="w-full max-w-sm text-left"
              dismissible={false}
            >
              <p>{filterEmptyState.detail}</p>
            </PreferenceGuideDisclosure>
            {activeFilter !== 'all' && (
              <button
                type="button"
                aria-label="Show all updates"
                onClick={() => setActiveFilter('all')}
                className={cn(uiStyles.subtleButton, 'mt-2')}
              >
                Show all updates
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

function inboxFilterEmptyState(filter: InboxFilter): InboxFilterEmptyState {
  switch (filter) {
    case 'unread':
      return {
        title: 'No unread updates',
        detail: 'Older updates are still in All. Open All if you need the full history.',
      }
    case 'needs-action':
      return {
        title: 'You are caught up on action items',
        detail:
          'No task is asking for help and no account access needs reconnecting. Open All when you want to check older updates.',
      }
    case 'credentials':
      return {
        title: 'No account access needs reconnecting',
        detail:
          'No tasks are blocked by account access right now. Open All to check other updates.',
      }
    case 'all':
      return {
        title: 'No updates match this filter',
        detail: 'Open All if a filter is selected, or check Inbox again later for new updates.',
      }
  }
}

function matchingUpdatesLabel(count: number): string {
  return `${count} matching ${count === 1 ? 'update' : 'updates'}`
}

function isActionNotification(notification: Notification): boolean {
  return (
    notification.type === 'blocked' ||
    notification.type === 'failed' ||
    notification.type === 'credential_expired' ||
    notification.type === 'review_escalated'
  )
}

function matchesFilter(notification: Notification, filter: InboxFilter): boolean {
  switch (filter) {
    case 'all':
      return true
    case 'unread':
      return !notification.read
    case 'needs-action':
      return isActionNotification(notification)
    case 'credentials':
      return notification.type === 'credential_expired'
  }
}

function nextStepTitle(notification: Notification): string {
  if (notification.read && isActionNotification(notification)) {
    return 'No unread action items'
  }

  switch (notification.type) {
    case 'credential_expired':
      return 'Reconnect account access before agents continue tasks'
    case 'blocked':
      return 'Check what is stopping work'
    case 'failed':
      return 'Check the retry steps before retrying'
    case 'completed':
      return 'Open the latest completed result when you have time'
    case 'assigned':
      return 'Open the newest task update'
    case 'mentioned':
      return 'Open the newest mention'
    case 'cli_image_updated':
      return 'Check the latest agent tool update'
    case 'review_escalated':
      return 'Approve or reject the overdue review'
  }
}

function nextStepDescription(
  notification: Notification,
  needsActionCount: number,
  credentialCount: number
): string {
  if (notification.read && isActionNotification(notification)) {
    return 'Everything that needed help is marked read. Open this older item only if you still need to check it.'
  }

  if (notification.type === 'credential_expired') {
    return credentialCount === 1
      ? 'One account connection needs reconnecting. Fixing it helps agents finish future tasks.'
      : `${credentialCount} account connections need reconnecting. Start here because access problems can block new tasks.`
  }

  if (
    notification.type === 'blocked' ||
    notification.type === 'failed' ||
    notification.type === 'review_escalated'
  ) {
    return needsActionCount === 1
      ? 'This is the only item that needs action. Open it and decide the next owner step.'
      : `${needsActionCount} items need action. Start with the newest item that needs help.`
  }

  return 'There are no urgent items that need help. Open this update only if you need to check the latest work.'
}

function nextStepActionLabel(notification: Notification): string {
  switch (notification.type) {
    case 'credential_expired':
      return 'Reconnect account access'
    case 'blocked':
      return 'Open task'
    case 'failed':
      return 'Check retry steps'
    case 'completed':
      return 'Open result'
    case 'assigned':
      return 'Open task update'
    case 'mentioned':
      return 'Open mention'
    case 'cli_image_updated':
      return 'Open tool updates'
    case 'review_escalated':
      return 'Open review'
  }
}

function InboxActionPath({
  compact = false,
  checkingSavedUpdates = false,
}: {
  compact?: boolean
  checkingSavedUpdates?: boolean
}) {
  return (
    <div data-testid="inbox-action-path" className={cn('w-full text-left', !compact && 'mt-3')}>
      <PreferenceGuideDisclosure
        guideKey="inbox-action-order"
        icon={<InboxIcon />}
        title="Inbox action order"
        dismissible={!compact}
      >
        {compact && (
          <div className="mb-2 space-y-1">
            <p>
              {checkingSavedUpdates
                ? 'Forge is checking older updates. New live updates will still appear here.'
                : 'Inbox updates appear after agents start work, finish work, need help, or ask you to reconnect account access.'}
            </p>
            {!checkingSavedUpdates && (
              <>
                <p>Next: start a task or wait for an agent update, then open Inbox again.</p>
                <p>Success looks like a new update listed here with the task name and next step.</p>
              </>
            )}
          </div>
        )}
        <ol className="list-decimal space-y-1 pl-4">
          {INBOX_ACTION_STEPS.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </PreferenceGuideDisclosure>
    </div>
  )
}
