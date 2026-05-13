import { useCallback } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import type { ComponentType, SVGProps } from 'react'
import {
  CheckSquare,
  Inbox,
  ClipboardCheck,
  Bot,
  Zap,
  BarChart3,
  BookOpenCheck,
  CreditCard,
  Settings,
  LogOut,
  Shield,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useAuth } from '@app/shared/model/auth.context'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useContextStore } from '@app/shared/model/context.store'

type IconComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>

interface NavItem {
  id: string
  Icon: IconComponent
  labelKey: string
  path: string
}

const NAV_ITEMS: NavItem[] = [
  { id: 'start', Icon: BookOpenCheck, labelKey: 'nav.start', path: '/start' },
  { id: 'tasks', Icon: CheckSquare, labelKey: 'nav.tasks', path: '/tasks' },
  { id: 'inbox', Icon: Inbox, labelKey: 'nav.inbox', path: '/inbox' },
  { id: 'context', Icon: ClipboardCheck, labelKey: 'nav.context', path: '/context' },
  { id: 'agents', Icon: Bot, labelKey: 'nav.agents', path: '/agents' },
  { id: 'skills', Icon: Zap, labelKey: 'nav.skills', path: '/skills' },
  { id: 'analytics', Icon: BarChart3, labelKey: 'nav.analytics', path: '/analytics' },
]

const BOTTOM_ITEMS: NavItem[] = [
  { id: 'billing', Icon: CreditCard, labelKey: 'nav.billing', path: '/billing' },
  { id: 'settings', Icon: Settings, labelKey: 'nav.settings', path: '/settings' },
]

interface SidebarNavProps {
  expanded: boolean
  activePath: string
  onNavigate: (path: string) => void
  section?: 'primary' | 'secondary'
}

export function SidebarNav({
  expanded,
  activePath,
  onNavigate,
  section = 'primary',
}: SidebarNavProps) {
  const { authManager, user } = useAuth()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const isAdmin = user?.role === 'admin'
  const contextGovernanceEnabled = useContextFeaturesStore((s) => s.governance)
  const pendingContextCount = useContextStore((s) => s.pendingCandidateCount)

  const handleLogout = useCallback(() => {
    authManager.logout()
    void navigate({ to: '/login', search: {} })
  }, [authManager, navigate])

  function renderItem(item: NavItem) {
    const active = activePath.startsWith(item.path)
    const label = t(item.labelKey)
    const Icon = item.Icon
    const badgeCount = item.id === 'context' ? pendingContextCount : 0
    return (
      <button
        key={item.id}
        data-testid={`sidebar-nav-${item.id}`}
        onClick={() => onNavigate(item.path)}
        className={cn(
          'relative flex items-center gap-2.5 rounded-lg transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          active
            ? 'bg-apple-blue/10 text-apple-blue shadow-[inset_0_0_0_1px_rgba(0,102,204,0.24)]'
            : 'text-foreground-light/80 dark:text-foreground-dark/80 hover:bg-black/[0.04] dark:hover:bg-white/[0.06] hover:text-foreground-light dark:hover:text-foreground-dark'
        )}
        title={label}
      >
        <Icon size={16} strokeWidth={2} className="flex-shrink-0" />
        {expanded && <span className="truncate text-ui-body font-medium">{label}</span>}
        {badgeCount > 0 && (
          <span
            data-testid="context-approval-nav-badge"
            className={cn(
              'min-w-5 h-5 px-1.5 rounded-full bg-apple-blue text-white text-ui-caption font-semibold leading-5 text-center',
              expanded ? 'ml-auto' : 'absolute -right-1 -top-1'
            )}
          >
            {badgeCount > 99 ? '99+' : badgeCount}
          </span>
        )}
      </button>
    )
  }

  if (section === 'primary') {
    const items = NAV_ITEMS.filter((item) => item.id !== 'context' || contextGovernanceEnabled)
    return (
      <div className={cn('flex flex-col gap-0.5', expanded ? 'px-2' : 'px-1.5 items-center')}>
        {items.map(renderItem)}
      </div>
    )
  }

  return (
    <div
      className={cn('flex flex-col gap-0.5', expanded ? 'px-2 pb-2' : 'px-1.5 pb-2 items-center')}
    >
      {BOTTOM_ITEMS.map(renderItem)}
      {isAdmin && renderItem({ id: 'admin', Icon: Shield, labelKey: 'nav.admin', path: '/admin' })}
      <button
        data-testid="sidebar-nav-logout"
        onClick={handleLogout}
        className={cn(
          'flex items-center gap-2.5 rounded-lg transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          'text-foreground-light/80 dark:text-foreground-dark/80 hover:bg-red-500/10 hover:text-red-500'
        )}
        title={t('nav.logout')}
      >
        <LogOut size={16} strokeWidth={2} className="flex-shrink-0" />
        {expanded && <span className="truncate text-ui-body font-medium">{t('nav.logout')}</span>}
      </button>
    </div>
  )
}
