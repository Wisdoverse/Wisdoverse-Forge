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
  { id: 'agents', Icon: Bot, label: 'Managed agents', path: '/agents' },
  { id: 'skills', Icon: Brain, label: 'Saved instructions', path: '/skills' },
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
        'flex flex-col items-center w-[52px] py-3 gap-2',
        'bg-surface backdrop-blur-xl rounded-panel',
        'shadow-card dark:bg-surface-dark dark:shadow-card-dark'
      )}
    >
      <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-apple-blue to-apple-purple flex items-center justify-center text-white text-sm font-bold mb-2">
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
        'w-9 h-9 rounded-lg flex items-center justify-center transition-colors',
        active
          ? 'bg-apple-blue/10 text-apple-blue shadow-[inset_0_0_0_1.5px_rgba(0,122,255,0.3)]'
          : 'bg-black/[0.04] text-secondary-light dark:bg-white/[0.06] dark:text-secondary-dark hover:bg-black/[0.08] hover:text-foreground-light dark:hover:bg-white/[0.1] dark:hover:text-foreground-dark'
      )}
      title={item.label}
      aria-label={item.label}
      aria-current={active ? 'page' : undefined}
    >
      <Icon size={17} strokeWidth={2.1} aria-hidden="true" />
    </button>
  )
}
