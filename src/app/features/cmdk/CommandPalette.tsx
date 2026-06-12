import { Command } from 'cmdk'
import { cn } from '@app/shared/lib/utils'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onSelect?: (command: string) => void
}

const NAV_COMMANDS = [
  { id: 'nav:tasks', label: 'Tasks', description: 'See work that is planned, active, or done.' },
  { id: 'nav:inbox', label: 'Inbox', description: 'Review alerts that may need a person.' },
  {
    id: 'nav:context',
    label: 'Context',
    description: 'Review knowledge before agents use it in tasks.',
  },
  { id: 'nav:agents', label: 'Agents', description: 'Create or check agents that handle work.' },
  { id: 'nav:skills', label: 'Skills', description: 'Reuse instructions for repeated work.' },
  {
    id: 'nav:settings',
    label: 'Settings',
    description: 'Connect tools, keys, teams, and projects.',
  },
]

const ACTION_COMMANDS = [
  { id: 'action:create-task', label: 'Create task', description: 'Start a new piece of work.' },
  { id: 'action:toggle-theme', label: 'Change theme', description: 'Switch the app appearance.' },
]

const VIEW_COMMANDS = [
  { id: 'view:board', label: 'Board view', description: 'Move tasks through simple columns.' },
  { id: 'view:list', label: 'List view', description: 'Scan tasks in one sortable table.' },
  { id: 'view:timeline', label: 'Timeline view', description: 'See when work happened.' },
  { id: 'view:3d', label: 'Visual map', description: 'See agents and tasks on a visual map.' },
]

const COMMAND_DISCOVERY_STEPS = [
  'Use Tasks when you want to plan or inspect work.',
  'Use Inbox when something needs your attention.',
  'Use Settings when setup, account access, or agent work status is blocking work.',
]

export function CommandPalette({ isOpen, onClose, onSelect }: CommandPaletteProps) {
  const contextGovernanceEnabled = useContextFeaturesStore((s) => s.governance)
  if (!isOpen) return null
  const navCommands = NAV_COMMANDS.filter(
    (cmd) => cmd.id !== 'nav:context' || contextGovernanceEnabled
  )

  function handleSelect(commandId: string) {
    onSelect?.(commandId)
    onClose()
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className={cn(
          'w-full max-w-lg mx-4',
          'bg-surface dark:bg-surface-dark',
          'rounded-[10px] shadow-panel',
          'overflow-hidden'
        )}
      >
        <Command>
          <div className="border-b border-black/[0.08] px-4 py-3 dark:border-white/[0.08]">
            <p className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
              Command discovery path
            </p>
            <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {COMMAND_DISCOVERY_STEPS.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
          <Command.Input
            placeholder="Search commands, e.g. tasks, inbox, settings"
            className={cn(
              'w-full px-4 py-3 text-sm outline-none',
              'bg-transparent border-b border-black/[0.08] dark:border-white/[0.08]',
              'text-foreground-light dark:text-foreground-dark',
              'placeholder:text-secondary-light dark:placeholder:text-secondary-dark'
            )}
          />
          <Command.List className="max-h-80 overflow-y-auto py-2">
            <Command.Empty className="px-4 py-6 text-center text-sm text-secondary-light dark:text-secondary-dark">
              <p className="font-medium text-foreground-light dark:text-foreground-dark">
                No command matches that search
              </p>
              <p className="mt-1">
                Try Tasks, Inbox, Agents, Skills, or Settings to jump to a common workflow.
              </p>
              <p className="mt-1 text-ui-caption">
                Clear the search if you are not sure what to type; the full command list will come
                back.
              </p>
            </Command.Empty>

            <Command.Group
              heading="Go to a page"
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {navCommands.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.description}`}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex cursor-pointer flex-col gap-0.5 px-4 py-2 text-sm',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">{cmd.label}</span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {cmd.description}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>

            <Command.Group
              heading="Start an action"
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {ACTION_COMMANDS.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.description}`}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex cursor-pointer flex-col gap-0.5 px-4 py-2 text-sm',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">{cmd.label}</span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {cmd.description}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>

            <Command.Group
              heading="Change task view"
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {VIEW_COMMANDS.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.description}`}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex cursor-pointer flex-col gap-0.5 px-4 py-2 text-sm',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">{cmd.label}</span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {cmd.description}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>
          </Command.List>
        </Command>
      </div>
    </div>
  )
}
