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
  const disabledHelp = !selectedProjectId
    ? 'Choose a project before choosing where tasks wait.'
    : groups.length === 0
      ? 'Set up where tasks wait, then come back here.'
      : null
  const selectTitle = disabledHelp ?? 'Choose where new tasks should wait.'

  return (
    <div className="hidden items-center gap-2 rounded-full border border-black/[0.08] bg-white p-0.5 pl-3 dark:border-white/[0.1] dark:bg-white/[0.06] md:flex">
      <span className="shrink-0 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        Where tasks wait
      </span>
      <select
        aria-label="Where new tasks wait"
        title={selectTitle}
        value={selectedGroupId ?? ''}
        onChange={(event) => {
          if (event.target.value) onSelectGroup(event.target.value)
        }}
        disabled={!selectedProjectId || groups.length === 0}
        className={cn(
          'h-8 min-w-40 rounded-full bg-transparent px-3 text-ui-caption outline-none',
          'text-foreground-light dark:text-foreground-dark',
          'disabled:cursor-not-allowed disabled:text-secondary-light dark:disabled:text-secondary-dark'
        )}
      >
        {!selectedProjectId && <option value="">Choose a project first</option>}
        {selectedProjectId && groups.length === 0 && (
          <option value="">Set up where tasks wait first</option>
        )}
        {groups.map((group) => (
          <option key={group.id} value={group.id}>
            {group.name}
          </option>
        ))}
      </select>
    </div>
  )
}
