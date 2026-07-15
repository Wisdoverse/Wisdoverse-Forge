import { cn } from '@app/shared/lib/utils'
import { waitingPlaceDisplayName, type NavAgentGroup } from '@app/entities/navigation/agent-group'

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
    ? 'Choose a project before choosing where new tasks wait.'
    : groups.length === 0
      ? 'Set up a place where new tasks wait, then come back here.'
      : null
  const selectTitle = disabledHelp ?? 'Choose where new tasks wait.'

  return (
    <div className="hidden items-center gap-2 rounded-button border border-black/[0.08] bg-white p-0.5 pl-3 dark:border-white/[0.1] dark:bg-white/[0.06] md:flex">
      <span className="shrink-0 text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
        Place for new tasks
      </span>
      <select
        aria-label="Place for new tasks"
        title={selectTitle}
        value={selectedGroupId ?? ''}
        onChange={(event) => {
          if (event.target.value) onSelectGroup(event.target.value)
        }}
        disabled={!selectedProjectId || groups.length === 0}
        className={cn(
          'h-8 min-w-40 rounded-button bg-transparent px-3 text-ui-caption outline-none',
          'text-foreground-light dark:text-foreground-dark',
          'disabled:cursor-not-allowed disabled:text-secondary-light dark:disabled:text-secondary-dark'
        )}
      >
        {!selectedProjectId && <option value="">Choose a project first</option>}
        {selectedProjectId && groups.length === 0 && <option value="">Set up a place first</option>}
        {groups.map((group) => (
          <option key={group.id} value={group.id}>
            {waitingPlaceDisplayName(group.name)}
          </option>
        ))}
      </select>
    </div>
  )
}
