import { cn } from '@app/shared/lib/utils'
import type { NavAgentGroup } from '@app/entities/agent-group'

interface AgentGroupSelectorProps {
  groups: NavAgentGroup[]
  selectedGroupId: string | null
  selectedProjectId: string | null
  onSelectGroup: (groupId: string) => void
}

export function AgentGroupSelector({
  groups,
  selectedGroupId,
  selectedProjectId,
  onSelectGroup,
}: AgentGroupSelectorProps) {
  return (
    <div className="hidden items-center gap-1 rounded-full border border-black/[0.08] bg-white p-0.5 dark:border-white/[0.1] dark:bg-white/[0.06] md:flex">
      <select
        aria-label="Task group"
        value={selectedGroupId ?? ''}
        onChange={(event) => {
          if (event.target.value) onSelectGroup(event.target.value)
        }}
        disabled={!selectedProjectId || groups.length === 0}
        className={cn(
          'h-8 min-w-36 rounded-full bg-transparent px-3 text-ui-caption outline-none',
          'text-foreground-light dark:text-foreground-dark',
          'disabled:cursor-not-allowed disabled:text-secondary-light dark:disabled:text-secondary-dark'
        )}
      >
        {!selectedProjectId && <option value="">No project</option>}
        {selectedProjectId && groups.length === 0 && <option value="">No task groups</option>}
        {groups.map((group) => (
          <option key={group.id} value={group.id}>
            {group.name}
          </option>
        ))}
      </select>
    </div>
  )
}
