import {
  AlertTriangle,
  ArrowRight,
  AtSign,
  CheckCircle2,
  KeyRound,
  RefreshCw,
  XCircle,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { isRawTaskFailureDetail } from '@app/shared/lib/taskFailureCopy'
import type { Notification } from '@app/shared/model/feed.store'

const TYPE_CONFIG: Record<
  Notification['type'],
  {
    Icon: LucideIcon
    color: string
    unreadBg: string
    dot: string
    label: string
    actionLabel: string
    guidance: string
    template: 'task-lifecycle' | 'credential-action' | 'collaboration'
  }
> = {
  blocked: {
    Icon: AlertTriangle,
    color: 'text-apple-red',
    unreadBg: 'bg-apple-red/[0.04]',
    dot: 'bg-apple-red',
    label: 'Needs help',
    actionLabel: 'Review what needs help',
    guidance: 'Open the task, read what is missing, and provide the requested input.',
    template: 'task-lifecycle',
  },
  completed: {
    Icon: CheckCircle2,
    color: 'text-secondary-light dark:text-secondary-dark',
    unreadBg: 'bg-apple-blue/[0.04]',
    dot: 'bg-apple-blue',
    label: 'Completed task',
    actionLabel: 'Review result',
    guidance: 'Open the result when you need to review, share, or reuse the work.',
    template: 'task-lifecycle',
  },
  failed: {
    Icon: XCircle,
    color: 'text-apple-red',
    unreadBg: 'bg-apple-red/[0.05]',
    dot: 'bg-apple-red',
    label: 'Recovery needed',
    actionLabel: 'Review recovery',
    guidance: 'Open the task, review the recovery note, then retry or choose another agent.',
    template: 'task-lifecycle',
  },
  assigned: {
    Icon: ArrowRight,
    color: 'text-apple-blue',
    unreadBg: 'bg-apple-blue/[0.05]',
    dot: 'bg-apple-blue',
    label: 'Assignment',
    actionLabel: 'Open task',
    guidance: 'Open the task to check what changed and who owns the next step.',
    template: 'collaboration',
  },
  mentioned: {
    Icon: AtSign,
    color: 'text-apple-gray-1',
    unreadBg: 'bg-apple-gray-1/[0.05]',
    dot: 'bg-apple-gray-1',
    label: 'Mention',
    actionLabel: 'Open thread',
    guidance: 'Open the thread to see where your input is needed.',
    template: 'collaboration',
  },
  credential_expired: {
    Icon: KeyRound,
    color: 'text-apple-blue',
    unreadBg: 'bg-apple-blue/[0.04]',
    dot: 'bg-apple-blue',
    label: 'Account access',
    actionLabel: 'Reconnect work access',
    guidance: 'Open agent work settings and reconnect the account agents use for file work.',
    template: 'credential-action',
  },
  cli_image_updated: {
    Icon: RefreshCw,
    color: 'text-apple-blue',
    unreadBg: 'bg-apple-blue/[0.04]',
    dot: 'bg-apple-blue',
    label: 'Tool update',
    actionLabel: 'Open tool updates',
    guidance:
      'Open Admin, then Agent tool updates, to check each work tool. New agents use the update automatically.',
    template: 'task-lifecycle',
  },
}

const RELATIVE_TIME = new Intl.RelativeTimeFormat('en-US', { numeric: 'auto' })

export function InboxItem({
  notification,
  onClick,
}: {
  notification: Notification
  onClick?: () => void
}) {
  const config = TYPE_CONFIG[notification.type]
  const Icon = config.Icon
  const title = displayNotificationTitle(notification)
  const message = displayNotificationMessage(notification)

  return (
    <button
      type="button"
      data-testid={`inbox-notification-${notification.id}`}
      data-template={config.template}
      aria-label={`${config.actionLabel}: ${title}`}
      onClick={onClick}
      className={cn(
        'flex w-full gap-3 px-4 py-3 text-left transition-colors',
        'hover:bg-black/[0.025] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:hover:bg-white/[0.04]',
        !notification.read && config.unreadBg
      )}
    >
      <div
        className={cn(
          'mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg',
          config.color,
          'bg-black/[0.035] dark:bg-white/[0.05]'
        )}
      >
        <Icon size={16} strokeWidth={2} aria-hidden="true" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              'shrink-0 rounded-full border px-2 py-0.5 text-ui-caption font-medium',
              config.color,
              'border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-white/[0.04]'
            )}
          >
            {config.label}
          </span>
          <span className="truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
            {formatTime(notification.timestamp)}
          </span>
          {!notification.read && <div className={cn('h-1.5 w-1.5 rounded-full', config.dot)} />}
        </div>
        <p
          className={cn(
            'mt-1 break-words text-ui-body font-medium text-foreground-light dark:text-foreground-dark',
            !notification.read && 'font-semibold'
          )}
        >
          {title}
        </p>
        <p className="mt-0.5 line-clamp-2 break-words text-ui-caption text-secondary-light dark:text-secondary-dark">
          {message}
        </p>
        <p className="mt-1 text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
          {config.guidance}
        </p>
        <span className={cn('mt-1 inline-flex text-ui-caption font-medium', config.color)}>
          {config.actionLabel}
        </span>
      </div>
    </button>
  )
}

function displayNotificationMessage(notification: Notification): string {
  if (notification.type === 'credential_expired') {
    return notification.message
      .replace(/\baccount connection\b/gi, 'work account')
      .replace(/\bruntime access\b/gi, 'agent work access')
      .replace(/\bcredentials?\b/gi, 'account access')
      .replace(/\bexpired\b/gi, 'needs reconnecting')
  }
  if (notification.type === 'failed') {
    return failedNotificationMessage(notification.message)
  }
  return notification.message
}

function failedNotificationMessage(message: string): string {
  if (
    !message.toLowerCase().includes('failed to complete this task') &&
    !isRawTaskFailureDetail(message)
  ) {
    return message
  }

  return 'The task stopped before finishing. Open it, review the recovery note, then retry or choose another agent.'
}

function displayNotificationTitle(notification: Notification): string {
  if (notification.type === 'credential_expired') {
    const title = notification.taskTitle
      .replace(/\bcredential expired\b/gi, 'account access needs reconnecting')
      .replace(/\bcredentials?\b/gi, 'account access')
      .replace(/\bexpired\b/gi, 'needs reconnecting')
    return title.charAt(0).toUpperCase() + title.slice(1)
  }
  return notification.taskTitle
}

function formatTime(ts: number): string {
  const diff = Math.max(0, Date.now() - ts)
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'now'
  if (mins < 60) return RELATIVE_TIME.format(-mins, 'minute')
  const hours = Math.floor(mins / 60)
  if (hours < 24) return RELATIVE_TIME.format(-hours, 'hour')
  return RELATIVE_TIME.format(-Math.floor(hours / 24), 'day')
}
