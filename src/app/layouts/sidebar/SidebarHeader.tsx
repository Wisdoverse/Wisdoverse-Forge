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
      <button
        data-testid={expanded ? 'sidebar-logo' : 'sidebar-toggle'}
        onClick={expanded ? undefined : onToggle}
        aria-label={expanded ? 'Wisdoverse Forge' : 'Expand sidebar'}
        className={cn(
          'flex h-8 w-8 items-center justify-center rounded-lg bg-apple-blue',
          'text-sm font-bold text-white transition-transform active:scale-95',
          !expanded && 'cursor-pointer hover:bg-apple-blue-focus'
        )}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M13 2L3 14h7l-1 8 10-12h-7l1-8z" />
        </svg>
      </button>
      {expanded && (
        <button
          data-testid="sidebar-toggle"
          onClick={onToggle}
          aria-label="Collapse sidebar"
          className={cn(
            'w-7 h-7 flex items-center justify-center rounded-md',
            'text-secondary-light dark:text-secondary-dark',
            'hover:bg-black/[0.06] dark:hover:bg-white/[0.08] hover:text-foreground-light dark:hover:text-foreground-dark',
            'transition-colors'
          )}
        >
          <PanelLeftClose size={15} strokeWidth={2} />
        </button>
      )}
    </div>
  )
}
