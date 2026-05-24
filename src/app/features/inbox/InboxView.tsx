import { useEffect, useMemo, useState } from 'react'
import { Inbox as InboxIcon } from 'lucide-react'
import { useNavigate } from '@tanstack/react-router'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { useFeedStore, type Notification } from '@app/shared/model/feed.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { cn } from '@app/shared/lib/utils'
import { InboxItem } from './InboxItem'

type InboxFilter = 'all' | 'unread' | 'needs-action' | 'credentials'

const FILTERS: { id: InboxFilter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'unread', label: 'Unread' },
  { id: 'needs-action', label: 'Needs action' },
  { id: 'credentials', label: 'Credentials' },
]

export function InboxView() {
  const { notifications, addNotification, markRead, markAllRead } = useFeedStore()
  const setSelectedTask = useBoardStore((s) => s.setSelectedTask)
  const navigate = useNavigate()
  const [activeFilter, setActiveFilter] = useState<InboxFilter>('all')
  const unreadCount = notifications.filter((n) => !n.read).length
  const orderedNotifications = useMemo(
    () => [...notifications].sort((a, b) => b.timestamp - a.timestamp),
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

  useEffect(() => {
    let cancelled = false
    orchestrationApi
      .fetchInboxNotifications()
      .then((items) => {
        if (cancelled) return
        items.forEach((item) => addNotification(item))
      })
      .catch((error) => {
        console.warn('Failed to load inbox notifications', error)
      })
    return () => {
      cancelled = true
    }
  }, [addNotification])

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
      <div className="flex flex-col items-center justify-center h-full gap-4 max-w-sm mx-auto text-center px-6">
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
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06]">
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
              Mark All Read
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
      </div>
      <div className="flex-1 divide-y divide-black/[0.04] overflow-y-auto dark:divide-white/[0.04]">
        {filteredNotifications.length > 0 ? (
          filteredNotifications.map((n) => (
            <InboxItem key={n.id} notification={n} onClick={() => handleOpenNotification(n)} />
          ))
        ) : (
          <div
            data-testid="inbox-filter-empty"
            className="flex h-full items-center justify-center px-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark"
          >
            No notifications in this view.
          </div>
        )}
      </div>
    </div>
  )
}

function matchesFilter(notification: Notification, filter: InboxFilter): boolean {
  switch (filter) {
    case 'all':
      return true
    case 'unread':
      return !notification.read
    case 'needs-action':
      return (
        notification.type === 'blocked' ||
        notification.type === 'failed' ||
        notification.type === 'credential_expired'
      )
    case 'credentials':
      return notification.type === 'credential_expired'
  }
}
