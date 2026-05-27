import type { AttentionItem } from '@app/shared/model/feed.store'

interface AttentionZoneProps {
  items: AttentionItem[]
  onApprove?: (id: string) => void
  onView?: (id: string) => void
}

export function AttentionZone({ items, onApprove, onView }: AttentionZoneProps) {
  if (items.length === 0) return null

  return (
    <div data-testid="attention-zone" className="mb-3 rounded-lg bg-apple-red/[0.04] p-3">
      <div className="mb-2">
        <div className="text-[10px] font-semibold text-apple-red">Needs your decision</div>
        <p className="mt-0.5 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
          Review each request before approving so the agent can continue safely.
        </p>
      </div>
      <p className="mb-2 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
        These items are waiting for a decision, missing access, or a quick review.
      </p>
      {items.map((item) => (
        <div
          key={item.id}
          className="mb-2 rounded-[10px] border-l-[3px] border-l-apple-red bg-white p-3 shadow-card last:mb-0 dark:bg-[#2c2c2e] dark:shadow-card-dark"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="min-w-0 truncate text-xs font-semibold">{item.taskTitle}</span>
            <span className="text-[9px] text-apple-red">{formatTime(item.timestamp)}</span>
          </div>
          <div className="mt-1 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
            {item.agentName} is waiting: {item.reason}
          </div>
          <div className="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => onView?.(item.id)}
              className="rounded-badge bg-black/[0.04] px-2.5 py-1 text-[9px] font-medium dark:bg-white/[0.06]"
            >
              Review request
            </button>
            <button
              type="button"
              onClick={() => onApprove?.(item.id)}
              className="rounded-badge bg-apple-blue px-2.5 py-1 text-[9px] font-medium text-white"
            >
              Approve request
            </button>
          </div>
        </div>
      ))}
    </div>
  )
}

function formatTime(ts: number): string {
  const diff = Date.now() - ts
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'now'
  if (mins < 60) return `${mins}m ago`
  return `${Math.floor(mins / 60)}h ago`
}
