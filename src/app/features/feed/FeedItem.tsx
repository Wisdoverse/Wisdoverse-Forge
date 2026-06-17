import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Circle,
  CircleDot,
  RefreshCw,
  XCircle,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import { isRawTaskFailureDetail, taskBlockedPreview } from '@app/shared/lib/taskFailureCopy'
import type { FeedItem as FeedItemType } from '@app/shared/model/feed.store'

const TYPE_ICONS: Record<string, LucideIcon> = {
  'task.completed': CheckCircle2,
  'task.queued': ArrowRight,
  'task.working': CircleDot,
  'task.blocked': AlertTriangle,
  'task.failed': XCircle,
  'task.progress': RefreshCw,
}

const TYPE_COPY: Record<string, { label: string; description: string }> = {
  'task.completed': {
    label: 'Finished',
    description: 'The task finished successfully.',
  },
  'task.queued': {
    label: 'Waiting',
    description: 'The task is waiting for an agent to start work.',
  },
  'task.working': {
    label: 'Working now',
    description: 'The agent is actively working on this task.',
  },
  'task.blocked': {
    label: 'Needs help',
    description: 'The task is waiting for someone to provide what is needed.',
  },
  'task.failed': {
    label: 'Review recovery',
    description: 'The task stopped before finishing and needs a retry decision.',
  },
  'task.progress': {
    label: 'Update',
    description: 'The agent shared progress on this task.',
  },
}

const TYPE_COLORS: Record<string, string> = {
  'task.completed': 'bg-apple-green/12 text-apple-green',
  'task.queued': 'bg-apple-orange/12 text-apple-orange',
  'task.working': 'bg-apple-blue/12 text-apple-blue',
  'task.blocked': 'bg-apple-red/12 text-apple-red',
  'task.failed': 'bg-apple-red/12 text-apple-red',
  'task.progress': 'bg-apple-blue/12 text-apple-blue',
}

const NEXT_ACTION_COPY: Record<string, { text: string; className: string }> = {
  'task.queued': {
    text: 'Next step: keep this task open. If it stays waiting, start an available agent or choose another one.',
    className: 'bg-apple-orange/[0.08] text-apple-orange',
  },
  'task.blocked': {
    text: 'Next step: open the task and provide what is missing or reconnect access.',
    className: 'bg-apple-red/[0.06] text-apple-red',
  },
  'task.failed': {
    text: 'Next step: open the task, follow the recovery note, then retry or choose another agent.',
    className: 'bg-apple-red/[0.06] text-apple-red',
  },
}

export function FeedItem({ item }: { item: FeedItemType }) {
  const Icon = TYPE_ICONS[item.type] ?? Circle
  const typeCopy = TYPE_COPY[item.type] ?? {
    label: 'Update',
    description: 'The agent shared a task update.',
  }
  const nextAction = NEXT_ACTION_COPY[item.type]
  const detail = displayFeedDetail(item)

  return (
    <article
      aria-label={`${typeCopy.label}: ${item.agentName} on ${item.taskTitle}. ${typeCopy.description}`}
      className="flex gap-2 py-2"
    >
      <div
        className={cn(
          'w-5 h-5 rounded-md flex items-center justify-center text-[10px] flex-shrink-0 mt-0.5',
          TYPE_COLORS[item.type] ?? 'bg-apple-gray-5 text-apple-gray-1'
        )}
      >
        <Icon size={12} strokeWidth={2.1} aria-hidden="true" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex min-w-0 items-center gap-1.5 text-[11px]">
          <span className="font-medium">{item.agentName}</span>
          <span className="text-secondary-light dark:text-secondary-dark"> · </span>
          <span className="min-w-0 flex-1 truncate font-medium">{item.taskTitle}</span>
          <span className="shrink-0 rounded-full bg-black/[0.04] px-1.5 py-0.5 text-[9px] font-medium text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark">
            {typeCopy.label}
          </span>
        </div>
        {detail && (
          <div className="text-[10px] text-secondary-light dark:text-secondary-dark mt-0.5">
            {detail}
          </div>
        )}
        {nextAction && (
          <div
            className={cn(
              'mt-1 rounded-md px-2 py-1 text-[10px] leading-relaxed',
              nextAction.className
            )}
          >
            {nextAction.text}
          </div>
        )}
        <div className="text-[9px] text-secondary-light dark:text-secondary-dark mt-0.5">
          {formatTime(item.timestamp)}
        </div>
      </div>
    </article>
  )
}

function displayFeedDetail(item: FeedItemType): string {
  if (item.type === 'task.blocked') {
    return taskBlockedPreview({ blockedHint: item.detail })
  }
  if (item.type !== 'task.failed') return item.detail

  if (!isRawTaskFailureDetail(item.detail)) return item.detail

  return 'Open details to see the recovery note, then retry or choose another agent.'
}

function formatTime(ts: number): string {
  return formatRelativeTime(new Date(ts).toISOString())
}
