import { useEffect } from 'react'
import { Inbox as InboxIcon } from 'lucide-react'
import { useNavigate } from '@tanstack/react-router'
import { orchestrationApi } from '@app/shared/api/orchestration'
import { useFeedStore } from '@app/shared/model/feed.store'
import { useBoardStore } from '@app/shared/model/board.store'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { InboxItem } from './InboxItem'

export function InboxView() {
  const { notifications, addNotification, markRead, markAllRead } = useFeedStore()
  const setSelectedTask = useBoardStore((s) => s.setSelectedTask)
  const navigate = useNavigate()
  const unreadCount = notifications.filter((n) => !n.read).length

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
      <div className="flex items-center justify-between border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06]">
        <div className="flex items-center gap-2">
          <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
            {notifications.length} {notifications.length === 1 ? 'notification' : 'notifications'}
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
      <div className="flex-1 divide-y divide-black/[0.04] overflow-y-auto dark:divide-white/[0.04]">
        {notifications.map((n) => (
          <InboxItem key={n.id} notification={n} onClick={() => handleOpenNotification(n)} />
        ))}
      </div>
    </div>
  )
}
