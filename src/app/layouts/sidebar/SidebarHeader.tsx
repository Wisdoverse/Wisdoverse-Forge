import { PanelLeftClose } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'

interface SidebarHeaderProps {
  expanded: boolean
  onToggle: () => void
}

export function SidebarHeader({ expanded, onToggle }: SidebarHeaderProps) {
  return (
    <div
      className={cn('flex items-center px-3 py-3', expanded ? 'justify-between' : 'justify-center')}
    >
      {expanded ? (
        <div data-testid="sidebar-logo" className="flex min-w-0 items-center gap-2">
          <LogoMark />
          <span className="truncate text-ui-body font-semibold text-foreground-light dark:text-foreground-dark">
            Wisdoverse Forge
          </span>
        </div>
      ) : (
        <button
          type="button"
          data-testid="sidebar-toggle"
          onClick={onToggle}
          aria-label="Expand sidebar"
          title="Expand sidebar"
          className={cn(
            'flex h-8 w-8 items-center justify-center rounded-lg bg-apple-blue',
            'text-sm font-bold text-white transition-transform active:scale-95',
            'cursor-pointer hover:bg-apple-blue-focus'
          )}
        >
          <LogoIcon />
        </button>
      )}
      {expanded && (
        <button
          type="button"
          data-testid="sidebar-toggle"
          onClick={onToggle}
          aria-label="Collapse sidebar"
          title="Collapse sidebar"
          className={cn(
            'w-7 h-7 flex items-center justify-center rounded-md',
            'text-secondary-light dark:text-secondary-dark',
            'hover:bg-black/[0.06] dark:hover:bg-white/[0.08] hover:text-foreground-light dark:hover:text-foreground-dark',
            'transition-colors'
          )}
        >
          <PanelLeftClose size={15} strokeWidth={2} aria-hidden="true" />
        </button>
      )}
    </div>
  )
}

function LogoMark() {
  return (
    <span
      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-apple-blue text-sm font-bold text-white"
      aria-hidden="true"
    >
      <LogoIcon />
    </span>
  )
}

function LogoIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M13 2L3 14h7l-1 8 10-12h-7l1-8z" />
    </svg>
  )
}
