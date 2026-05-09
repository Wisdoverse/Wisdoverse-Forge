import { cn } from '@app/shared/lib/utils'

interface NavItem {
  id: string
  icon: string
  label: string
  path: string
}

const NAV_ITEMS: NavItem[] = [
  { id: 'tasks', icon: '📋', label: 'Tasks', path: '/tasks' },
  { id: 'inbox', icon: '📥', label: 'Inbox', path: '/inbox' },
  { id: 'agents', icon: '🤖', label: 'Agents', path: '/agents' },
  { id: 'skills', icon: '🧠', label: 'Skills', path: '/skills' },
]

const BOTTOM_ITEMS: NavItem[] = [
  { id: 'settings', icon: '⚙️', label: 'Settings', path: '/settings' },
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
        ⚡
      </div>

      {NAV_ITEMS.map((item) => (
        <button
          key={item.id}
          data-testid={`nav-${item.id}`}
          onClick={() => onNavigate(item.path)}
          className={cn(
            'w-9 h-9 rounded-lg flex items-center justify-center text-base transition-colors',
            activePath.startsWith(item.path)
              ? 'bg-apple-blue/10 shadow-[inset_0_0_0_1.5px_rgba(0,122,255,0.3)]'
              : 'bg-black/[0.04] dark:bg-white/[0.06] hover:bg-black/[0.08] dark:hover:bg-white/[0.1]'
          )}
          title={item.label}
          aria-label={item.label}
        >
          {item.icon}
        </button>
      ))}

      <div className="flex-1" />
      <div className="w-6 h-px bg-black/[0.08] dark:bg-white/[0.08]" />

      {BOTTOM_ITEMS.map((item) => (
        <button
          key={item.id}
          data-testid={`nav-${item.id}`}
          onClick={() => onNavigate(item.path)}
          className={cn(
            'w-9 h-9 rounded-lg flex items-center justify-center text-base transition-colors',
            activePath.startsWith(item.path)
              ? 'bg-apple-blue/10 shadow-[inset_0_0_0_1.5px_rgba(0,122,255,0.3)]'
              : 'bg-black/[0.04] dark:bg-white/[0.06] hover:bg-black/[0.08] dark:hover:bg-white/[0.1]'
          )}
          title={item.label}
          aria-label={item.label}
        >
          {item.icon}
        </button>
      ))}
    </nav>
  )
}
