import { cn } from '@app/shared/lib/utils'
import type { AgentStatus } from '@app/shared/model/feed.store'

const STATUS_COLORS: Record<string, string> = {
  working: 'bg-apple-green',
  idle: 'bg-apple-gray-2',
  blocked: 'bg-apple-orange',
  offline: 'bg-apple-gray-3',
}

export function AgentStatusBar({ agents }: { agents: AgentStatus[] }) {
  if (agents.length === 0) return null

  return (
    <div data-testid="agent-status-bar" className="flex items-center gap-2 flex-wrap">
      {agents.map((agent) => (
        <div
          key={agent.id}
          className={cn(
            'flex items-center gap-1.5 px-2 py-1 rounded-lg text-[10px]',
            agent.status === 'blocked' ? 'bg-apple-red/8' : 'bg-apple-green/8'
          )}
        >
          <div className={cn('w-1.5 h-1.5 rounded-full', STATUS_COLORS[agent.status])} />
          <span className="font-medium">{agent.name}</span>
          <span className="text-secondary-light dark:text-secondary-dark">{agent.status}</span>
        </div>
      ))}
    </div>
  )
}
