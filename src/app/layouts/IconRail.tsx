import { Bot, Brain, ClipboardList, Inbox, Settings, Zap, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'

interface NavItem {
  id: string
  Icon: LucideIcon
  label: string
  path: string
}

const NAV_ITEMS: NavItem[] = [
  { id: 'tasks', Icon: ClipboardList, label: 'Task board', path: '/tasks' },
  { id: 'inbox', Icon: Inbox, label: 'Updates inbox', path: '/inbox' },
  { id: 'agents', Icon: Bot, label: 'Agents and chat services', path: '/agents' },
  { id: 'skills', Icon: Brain, label: 'Saved guidance', path: '/skills' },
]

const BOTTOM_ITEMS: NavItem[] = [
  { id: 'settings', Icon: Settings, label: 'Settings and setup', path: '/settings' },
]

interface IconRailProps {
  activePath: string
  onNavigate: (path: string) => void
}

export function IconRail({ activePath, onNavigate }: IconRailProps) {
  return (
    <nav
      data-testid="icon-rail"
      className={cn(
        'flex w-[52px] flex-col items-center gap-2 border-r border-black/[0.08] bg-background-light py-3',
        'dark:border-white/[0.1] dark:bg-background-dark'
      )}
    >
      <div className="mb-2 flex h-8 w-8 items-center justify-center rounded-button bg-apple-blue text-ui-body font-bold text-white">
        <Zap size={16} strokeWidth={2.2} aria-hidden="true" />
      </div>

      {NAV_ITEMS.map((item) => (
        <IconRailButton
          key={item.id}
          item={item}
          active={activePath.startsWith(item.path)}
          onNavigate={onNavigate}
        />
      ))}

      <div className="flex-1" />
      <div className="w-6 h-px bg-black/[0.08] dark:bg-white/[0.08]" />

      {BOTTOM_ITEMS.map((item) => (
        <IconRailButton
          key={item.id}
          item={item}
          active={activePath.startsWith(item.path)}
          onNavigate={onNavigate}
        />
      ))}
    </nav>
  )
}

function IconRailButton({
  item,
  active,
  onNavigate,
}: {
  item: NavItem
  active: boolean
  onNavigate: (path: string) => void
}) {
  const { Icon } = item

  return (
    <button
      type="button"
      data-testid={`nav-${item.id}`}
      onClick={() => onNavigate(item.path)}
      className={cn(
        'flex h-9 w-9 items-center justify-center rounded-button transition-colors',
        active
          ? 'rounded-button bg-black/[0.06] text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark'
          : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
      )}
      title={item.label}
      aria-label={item.label}
      aria-current={active ? 'page' : undefined}
    >
      <Icon size={16} strokeWidth={2.1} aria-hidden="true" />
    </button>
  )
}
