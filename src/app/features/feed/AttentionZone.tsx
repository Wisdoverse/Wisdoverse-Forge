import type { AttentionItem } from '@app/shared/model/feed.store'

interface AttentionZoneProps {
  items: AttentionItem[]
  onApprove?: (id: string) => void
  onView?: (id: string) => void
}

export function AttentionZone({ items, onApprove, onView }: AttentionZoneProps) {
  if (items.length === 0) return null

  return (
    <div data-testid="attention-zone" className="bg-apple-red/[0.04] rounded-lg p-3 mb-3">
      <div className="text-[9px] font-semibold text-apple-red tracking-wide mb-2">
        ACTION NEEDED
      </div>
      <p className="mb-2 text-[10px] leading-relaxed text-secondary-light dark:text-secondary-dark">
        These items are waiting for a decision, missing access, or a quick review.
      </p>
      {items.map((item) => (
        <div
          key={item.id}
          className="bg-white dark:bg-[#2c2c2e] rounded-[10px] p-3 shadow-card dark:shadow-card-dark border-l-[3px] border-l-apple-red mb-2 last:mb-0"
        >
          <div className="flex justify-between items-center">
            <span className="text-xs font-semibold">{item.taskTitle}</span>
            <span className="text-[9px] text-apple-red">{formatTime(item.timestamp)}</span>
          </div>
          <div className="text-[10px] text-secondary-light dark:text-secondary-dark mt-1">
            {item.agentName} — {item.reason}
          </div>
          <div className="flex gap-2 mt-2">
            <button
              onClick={() => onApprove?.(item.id)}
              className="text-[9px] font-medium px-2.5 py-1 rounded-badge bg-apple-blue text-white"
            >
              Approve now
            </button>
            <button
              onClick={() => onView?.(item.id)}
              className="text-[9px] font-medium px-2.5 py-1 rounded-badge bg-black/[0.04] dark:bg-white/[0.06]"
            >
              Review details
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
