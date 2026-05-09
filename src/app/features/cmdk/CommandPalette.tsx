import { Command } from 'cmdk'
import { cn } from '@app/shared/lib/utils'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onSelect?: (command: string) => void
}

const NAV_COMMANDS = [
  { id: 'nav:tasks', label: 'Tasks', icon: '✅' },
  { id: 'nav:inbox', label: 'Inbox', icon: '📥' },
  { id: 'nav:context', label: 'Context', icon: '☑️' },
  { id: 'nav:agents', label: 'Agents', icon: '🤖' },
  { id: 'nav:skills', label: 'Skills', icon: '⚡' },
  { id: 'nav:settings', label: 'Settings', icon: '⚙️' },
]

const ACTION_COMMANDS = [
  { id: 'action:create-task', label: 'Create Task', icon: '➕' },
  { id: 'action:toggle-theme', label: 'Toggle Theme', icon: '🌓' },
]

const VIEW_COMMANDS = [
  { id: 'view:board', label: 'Board', icon: '📋' },
  { id: 'view:list', label: 'List', icon: '📝' },
  { id: 'view:timeline', label: 'Timeline', icon: '📅' },
  { id: 'view:3d', label: '3D', icon: '🎮' },
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
          <Command.Input
            placeholder="Search commands..."
            className={cn(
              'w-full px-4 py-3 text-sm outline-none',
              'bg-transparent border-b border-black/[0.08] dark:border-white/[0.08]',
              'text-foreground-light dark:text-foreground-dark',
              'placeholder:text-secondary-light dark:placeholder:text-secondary-dark'
            )}
          />
          <Command.List className="max-h-80 overflow-y-auto py-2">
            <Command.Empty className="px-4 py-6 text-center text-sm text-secondary-light dark:text-secondary-dark">
              No commands found.
            </Command.Empty>

            <Command.Group
              heading="Navigation"
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {navCommands.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={cmd.id}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex items-center gap-3 px-4 py-2 text-sm cursor-pointer',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span>{cmd.icon}</span>
                  <span>{cmd.label}</span>
                </Command.Item>
              ))}
            </Command.Group>

            <Command.Group
              heading="Actions"
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {ACTION_COMMANDS.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={cmd.id}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex items-center gap-3 px-4 py-2 text-sm cursor-pointer',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span>{cmd.icon}</span>
                  <span>{cmd.label}</span>
                </Command.Item>
              ))}
            </Command.Group>

            <Command.Group
              heading="Views"
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {VIEW_COMMANDS.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={cmd.id}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex items-center gap-3 px-4 py-2 text-sm cursor-pointer',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span>{cmd.icon}</span>
                  <span>{cmd.label}</span>
                </Command.Item>
              ))}
            </Command.Group>
          </Command.List>
        </Command>
      </div>
    </div>
  )
}
