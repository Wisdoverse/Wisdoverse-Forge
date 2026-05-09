import { cn } from '@app/shared/lib/utils'
import { formatRelativeTime } from '@app/shared/lib/time'
import type { FeedItem as FeedItemType } from '@app/shared/model/feed.store'

const TYPE_ICONS: Record<string, string> = {
  'task.completed': '✓',
  'task.queued': '→',
  'task.working': '●',
  'task.blocked': '⚠',
  'task.failed': '✕',
  'task.progress': '↻',
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
  return (
    <div className="flex gap-2 py-2">
      <div
        className={cn(
          'w-5 h-5 rounded-md flex items-center justify-center text-[10px] flex-shrink-0 mt-0.5',
          TYPE_COLORS[item.type] ?? 'bg-apple-gray-5 text-apple-gray-1'
        )}
      >
        {TYPE_ICONS[item.type] ?? '•'}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-[11px]">
          <span className="font-medium">{item.agentName}</span>
          <span className="text-secondary-light dark:text-secondary-dark"> · </span>
          <span className="font-medium">{item.taskTitle}</span>
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
