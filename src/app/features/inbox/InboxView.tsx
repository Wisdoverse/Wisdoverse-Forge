import { useCallback, useEffect, useMemo, useState } from 'react'
import { Inbox as InboxIcon, RefreshCw } from 'lucide-react'
import { useNavigate } from '@tanstack/react-router'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { useFeedStore, type Notification } from '@app/shared/model/feed.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { cn } from '@app/shared/lib/utils'
import { InboxItem } from './InboxItem'

type InboxFilter = 'all' | 'unread' | 'needs-action' | 'credentials'

const FILTERS: { id: InboxFilter; label: string; empty: string }[] = [
  { id: 'all', label: 'All', empty: 'No notifications match this view.' },
  { id: 'unread', label: 'Unread', empty: 'Nothing new is waiting for you.' },
  {
    id: 'needs-action',
    label: 'Needs action',
    empty: 'No blockers, failures, or expired credentials need action right now.',
  },
  {
    id: 'credentials',
    label: 'Credentials',
    empty: 'No credentials need reconnecting right now.',
  },
]

const INBOX_TRIAGE_STEPS = [
  'Start with Needs action to find blocked tasks and failures.',
  'Use Credentials when an agent needs access reconnected.',
  'Mark items read after the task or setting has been handled.',
]

export function InboxView() {
  const { notifications, addNotification, markRead, markAllRead } = useFeedStore()
  const setSelectedTask = useBoardStore((s) => s.setSelectedTask)
  const navigate = useNavigate()
  const [activeFilter, setActiveFilter] = useState<InboxFilter>('all')
  const [loadError, setLoadError] = useState(false)
  const unreadCount = notifications.filter((n) => !n.read).length
  const orderedNotifications = useMemo(
    () => [...notifications].sort((a, b) => b.timestamp - a.timestamp),
    [notifications]
  )
  const needsActionCount = useMemo(
    () => notifications.filter((notification) => isActionNotification(notification)).length,
    [notifications]
  )
  const credentialCount = useMemo(
    () => notifications.filter((notification) => notification.type === 'credential_expired').length,
    [notifications]
  )
  const filteredNotifications = useMemo(
    () => orderedNotifications.filter((notification) => matchesFilter(notification, activeFilter)),
    [activeFilter, orderedNotifications]
  )
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
  const nextStepNotification = useMemo(
    () =>
      orderedNotifications.find((notification) => notification.type === 'credential_expired') ??
      orderedNotifications.find((notification) => notification.type === 'blocked') ??
      orderedNotifications.find((notification) => notification.type === 'failed') ??
      orderedNotifications.find((notification) => !notification.read) ??
      orderedNotifications[0],
    [orderedNotifications]
  )
  const activeFilterConfig = FILTERS.find((filter) => filter.id === activeFilter) ?? FILTERS[0]

  const loadNotifications = useCallback(() => {
    let cancelled = false
    setLoadError(false)
    orchestrationApi
      .fetchInboxNotifications()
      .then((items) => {
        if (cancelled) return
        items.forEach((item) => addNotification(item))
      })
      .catch((error) => {
        if (cancelled) return
        console.warn('Failed to load inbox notifications', error)
        setLoadError(true)
      })
    return () => {
      cancelled = true
    }
  }, [addNotification])

  useEffect(() => loadNotifications(), [loadNotifications])

  function handleOpenNotification(notification: (typeof notifications)[number]) {
    markRead(notification.id)
    void orchestrationApi.markInboxNotificationRead(notification.id).catch((error) => {
      console.warn('Failed to mark inbox notification read', error)
    })
    if (notification.taskHref === '/tasks') {
      setSelectedTask(notification.taskId)
      void navigate({ to: '/tasks' })
    } else if (notification.taskHref === '/settings') {
      useSettingsStore.getState().setActiveSection('runtime')
      void navigate({ to: '/settings/$section', params: { section: 'runtime' } })
    }
  }

  function handleMarkAllRead() {
    markAllRead()
    void orchestrationApi.markAllInboxNotificationsRead().catch((error) => {
      console.warn('Failed to mark inbox notifications read', error)
    })
  }

  if (notifications.length === 0) {
    return (
      <div className="mx-auto flex h-full max-w-sm flex-col items-center justify-center gap-4 px-6 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-apple-blue/10 text-apple-blue">
          <InboxIcon size={26} strokeWidth={1.75} aria-hidden="true" />
        </div>
        <div className="space-y-1">
          <p className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
            You're all caught up
          </p>
          <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
            Agent updates, task completions, and system alerts will show up here.
          </p>
        </div>
        <InboxTriagePath compact />
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
            Start with blockers and expired credentials. Completed work can wait until review time.
          </p>
        </header>
        {nextStepNotification && (
          <div
            data-testid="inbox-next-step"
            className="mb-3 rounded-card border border-black/[0.08] bg-black/[0.025] px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.03]"
          >
            <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
              <div className="min-w-0">
                <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                  Do This Next
                </p>
                <p className="mt-0.5 text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
                  {nextStepTitle(nextStepNotification)}
                </p>
                <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                  {nextStepDescription(nextStepNotification, needsActionCount, credentialCount)}
                </p>
              </div>
              <button
                type="button"
                onClick={() => handleOpenNotification(nextStepNotification)}
                className="inline-flex h-9 shrink-0 items-center justify-center rounded-full bg-apple-blue px-3 text-ui-button font-semibold text-white transition-colors hover:bg-apple-blue-focus focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
              >
                {nextStepActionLabel(nextStepNotification)}
              </button>
            </div>
          </div>
        )}
        {loadError && (
          <div
            role="alert"
            className="mb-3 flex flex-col gap-2 rounded-card border border-apple-red/20 bg-apple-red/10 px-3 py-2 text-ui-body text-apple-red sm:flex-row sm:items-center sm:justify-between"
          >
            <span>Could not load older notifications. New updates will still appear here.</span>
            <button
              type="button"
              onClick={loadNotifications}
              className="inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-ui-button font-semibold text-apple-red transition-colors hover:bg-apple-red/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-red/30"
            >
              <RefreshCw size={14} aria-hidden="true" />
              Try Again
            </button>
          </div>
        )}
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2">
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {filteredNotifications.length} of {notifications.length}{' '}
              {notifications.length === 1 ? 'notification' : 'notifications'}
            </p>
            {unreadCount > 0 && (
              <span
                data-testid="unread-count"
                className="rounded-full bg-apple-blue px-2 py-0.5 text-ui-caption font-medium text-white"
              >
                {unreadCount} new
              </span>
            )}
          </div>
          {unreadCount > 0 && (
            <button
              type="button"
              onClick={handleMarkAllRead}
              className="rounded-full px-3 py-1.5 text-ui-button font-medium text-apple-blue transition-colors hover:bg-apple-blue/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus"
            >
              Mark All As Read
            </button>
          )}
        </div>
        <div
          className="mt-3 flex flex-wrap gap-1 rounded-lg bg-black/[0.035] p-1 dark:bg-white/[0.05]"
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
                aria-label={`Filter by ${filter.label.toLowerCase()} notifications`}
                onClick={() => setActiveFilter(filter.id)}
                className={cn(
                  'flex min-h-8 items-center gap-1.5 rounded-md px-2.5 py-1 text-ui-button font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                  selected
                    ? 'bg-white text-foreground-light shadow-sm dark:bg-white/[0.12] dark:text-foreground-dark'
                    : 'text-secondary-light hover:bg-white/70 hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.08] dark:hover:text-foreground-dark'
                )}
              >
                <span>{filter.label}</span>
                <span
                  data-testid={`inbox-filter-count-${filter.id}`}
                  className={cn(
                    'rounded-full px-1.5 py-0.5 text-[10px] font-semibold tabular-nums',
                    selected
                      ? 'bg-apple-blue/10 text-apple-blue'
                      : 'bg-black/[0.06] text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark'
                  )}
                >
                  {filterCounts[filter.id]}
                </span>
              </button>
            )
          })}
        </div>
        <InboxTriagePath />
      </div>
      <div className="flex-1 divide-y divide-black/[0.04] overflow-y-auto dark:divide-white/[0.04]">
        {filteredNotifications.length > 0 ? (
          filteredNotifications.map((n) => (
            <InboxItem key={n.id} notification={n} onClick={() => handleOpenNotification(n)} />
          ))
        ) : (
          <div
            data-testid="inbox-filter-empty"
            className="flex h-full flex-col items-center justify-center px-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark"
          >
            <p>No notifications in this view.</p>
            <p className="mt-1 max-w-sm text-ui-caption">
              Try All for the full history, or Needs action for items that still need a response.
            </p>
          </div>
        )}
      </div>
    </div>
  )
}

function isActionNotification(notification: Notification): boolean {
  return (
    notification.type === 'blocked' ||
    notification.type === 'failed' ||
    notification.type === 'credential_expired'
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

function InboxTriagePath({ compact = false }: { compact?: boolean }) {
  return (
    <section
      data-testid="inbox-triage-path"
      className={cn(
        'text-left text-ui-caption text-secondary-light dark:text-secondary-dark',
        compact
          ? 'max-w-sm'
          : 'mt-3 rounded-lg border border-black/[0.06] bg-white px-3 py-2 dark:border-white/[0.08] dark:bg-white/[0.03]'
      )}
    >
      <p className="font-semibold text-foreground-light dark:text-foreground-dark">
        Inbox triage path
      </p>
      <ol className="mt-2 list-decimal space-y-1 pl-4">
        {INBOX_TRIAGE_STEPS.map((step) => (
          <li key={step}>{step}</li>
        ))}
      </ol>
    </section>
  )
}
