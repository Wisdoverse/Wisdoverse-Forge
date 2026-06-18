import { useState } from 'react'
import { Command } from 'cmdk'
import { cn } from '@app/shared/lib/utils'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useSettingsStore } from '@app/shared/model/settings.store'
import { shouldShowGettingStarted } from '@app/shared/lib/gettingStartedPreference'

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onSelect?: (command: string) => void
}

const NAV_COMMANDS = [
  {
    id: 'nav:start',
    label: 'Setup checklist',
    description: 'Review setup steps again when you want a guided checklist.',
  },
  { id: 'nav:tasks', label: 'Tasks', description: 'See work that is planned, active, or done.' },
  { id: 'nav:inbox', label: 'Inbox', description: 'Review alerts that may need a person.' },
  {
    id: 'nav:context',
    label: 'Saved items',
    description: 'Review saved notes and instructions before agents reuse them.',
  },
  { id: 'nav:agents', label: 'Agents', description: 'Create or check agents that handle work.' },
  {
    id: 'nav:skills',
    label: 'Saved instructions',
    description: 'Reuse instructions for repeated work.',
  },
  {
    id: 'nav:settings',
    label: 'Settings',
    description: 'Connect tools, account access, teams, and projects.',
  },
]

const ACTION_COMMANDS = [
  {
    id: 'action:create-task',
    label: 'New task',
    description: 'Create a task for an agent to finish.',
  },
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

function commonWorkflowSuggestion(commands: typeof NAV_COMMANDS): string {
  const labels = commands.map((command) => command.label)
  if (labels.length === 0) return 'Try a shorter search, or open Settings to browse setup.'
  if (labels.length === 1) return `Try ${labels[0]} to open a page people use often.`
  const prefix = labels.slice(0, -1).join(', ')
  return `Try ${prefix}, or ${labels[labels.length - 1]} to open a page people use often.`
}

export function CommandPalette({ isOpen, onClose, onSelect }: CommandPaletteProps) {
  const contextGovernanceEnabled = useContextFeaturesStore((s) => s.governance)
  const showGettingStarted = useSettingsStore((s) => shouldShowGettingStarted(s.preferences))
  const [search, setSearch] = useState('')
  if (!isOpen) return null
  const navCommands = NAV_COMMANDS.filter(
    (cmd) =>
      (cmd.id !== 'nav:context' || contextGovernanceEnabled) &&
      (cmd.id !== 'nav:start' || showGettingStarted)
  )
  const emptySearchSuggestion = commonWorkflowSuggestion(navCommands)

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
              Find what you need
            </p>
            <ol className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark">
              {COMMAND_DISCOVERY_STEPS.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
          <Command.Input
            value={search}
            onValueChange={setSearch}
            placeholder="Search pages or things to do, e.g. tasks, inbox, settings"
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
                No page or option matches that search
              </p>
              <p className="mt-1">{emptySearchSuggestion}</p>
              <button
                type="button"
                onClick={() => setSearch('')}
                className="mt-3 inline-flex h-8 items-center justify-center rounded-full border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.03] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]"
              >
                Clear search
              </button>
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
              heading="Create or change something"
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
