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
import type { FeedItem as FeedItemType } from '@app/shared/model/feed.store'

const TYPE_ICONS: Record<string, LucideIcon> = {
  'task.completed': CheckCircle2,
  'task.queued': ArrowRight,
  'task.working': CircleDot,
  'task.blocked': AlertTriangle,
  'task.failed': XCircle,
  'task.progress': RefreshCw,
}

const TYPE_LABELS: Record<string, string> = {
  'task.completed': 'Completed',
  'task.queued': 'Queued',
  'task.working': 'Working',
  'task.blocked': 'Blocked',
  'task.failed': 'Failed',
  'task.progress': 'Progress',
}

const TYPE_COLORS: Record<string, string> = {
  'task.completed': 'bg-apple-green/12 text-apple-green',
  'task.queued': 'bg-apple-orange/12 text-apple-orange',
  'task.working': 'bg-apple-blue/12 text-apple-blue',
  'task.blocked': 'bg-apple-red/12 text-apple-red',
  'task.failed': 'bg-apple-red/12 text-apple-red',
  'task.progress': 'bg-apple-blue/12 text-apple-blue',
}

export function FeedItem({ item }: { item: FeedItemType }) {
  const Icon = TYPE_ICONS[item.type] ?? Circle

  return (
    <div className="flex gap-2 py-2">
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
            {TYPE_LABELS[item.type] ?? 'Update'}
          </span>
        </div>
        {item.detail && (
          <div className="text-[10px] text-secondary-light dark:text-secondary-dark mt-0.5">
            {item.detail}
          </div>
        )}
        <div className="text-[9px] text-secondary-light dark:text-secondary-dark mt-0.5">
          {formatTime(item.timestamp)}
        </div>
      </div>
    </div>
  )
}

function formatTime(ts: number): string {
  return formatRelativeTime(new Date(ts).toISOString())
}
