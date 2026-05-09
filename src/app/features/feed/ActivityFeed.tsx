import { Activity } from 'lucide-react'
import { useFeedStore } from '@app/shared/model/feed.store'
import { AgentStatusBar } from './AgentStatusBar'
import { AttentionZone } from './AttentionZone'
import { FeedItem } from './FeedItem'

export function ActivityFeed() {
  const { agents, attentionItems, feedItems } = useFeedStore()

  return (
    <div className="flex flex-col gap-3">
      <AgentStatusBar agents={agents} />
      <AttentionZone items={attentionItems} />

      {feedItems.length > 0 ? (
        <div>
          <div className="text-[10px] font-semibold text-secondary-light dark:text-secondary-dark tracking-[0.08em] uppercase mb-2">
            Live feed
          </div>
          <div className="flex flex-col divide-y divide-black/[0.04] dark:divide-white/[0.04]">
            {feedItems.map((item) => (
              <FeedItem key={item.id} item={item} />
            ))}
          </div>
        </div>
      ) : (
        <div className="flex flex-col items-center text-center gap-2 py-10 px-2">
          <div className="w-10 h-10 rounded-full bg-black/[0.04] dark:bg-white/[0.06] text-secondary-light dark:text-secondary-dark flex items-center justify-center">
            <Activity size={18} strokeWidth={1.75} />
          </div>
          <p className="text-xs font-medium text-foreground-light dark:text-foreground-dark">
            Quiet so far
          </p>
          <p className="text-[11px] text-secondary-light dark:text-secondary-dark leading-relaxed">
            Tool calls, prompts, and agent updates will stream here in real time.
          </p>
        </div>
      )}
    </div>
  )
}
