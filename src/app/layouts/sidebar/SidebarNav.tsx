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
import { useSettingsStore } from '@app/shared/model/settings.store'

type IconComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>

interface NavItem {
  id: string
  Icon: IconComponent
  labelKey: string
  description: string
  path: string
}

const NAV_ITEMS: NavItem[] = [
  {
    id: 'start',
    Icon: BookOpenCheck,
    labelKey: 'nav.start',
    description: 'follow the setup checklist',
    path: '/start',
  },
  {
    id: 'tasks',
    Icon: CheckSquare,
    labelKey: 'nav.tasks',
    description: 'create and review agent work',
    path: '/tasks',
  },
  {
    id: 'inbox',
    Icon: Inbox,
    labelKey: 'nav.inbox',
    description: 'review items needing attention',
    path: '/inbox',
  },
  {
    id: 'context',
    Icon: ClipboardCheck,
    labelKey: 'nav.context',
    description: 'approve reusable knowledge',
    path: '/context',
  },
  {
    id: 'agents',
    Icon: Bot,
    labelKey: 'nav.agents',
    description: 'create and manage workers',
    path: '/agents',
  },
  {
    id: 'skills',
    Icon: Zap,
    labelKey: 'nav.skills',
    description: 'reuse proven work steps',
    path: '/skills',
  },
  {
    id: 'analytics',
    Icon: BarChart3,
    labelKey: 'nav.analytics',
    description: 'review usage and outcomes',
    path: '/analytics',
  },
]

const BOTTOM_ITEMS: NavItem[] = [
  {
    id: 'billing',
    Icon: CreditCard,
    labelKey: 'nav.billing',
    description: 'review plan and invoices',
    path: '/billing',
  },
  {
    id: 'settings',
    Icon: Settings,
    labelKey: 'nav.settings',
    description: 'configure workspace, runtime, and access',
    path: '/settings',
  },
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
  // Mirror the backend admin gate (`AdminService::require_admin`) and the /admin
  // route guard: owner AND admin can reach the admin console. Gating the nav link
  // on `admin` alone hid it from owners who can actually open /admin.
  const isAdmin = user?.role === 'admin' || user?.role === 'owner'
  const contextGovernanceEnabled = useContextFeaturesStore((s) => s.governance)
  const pendingContextCount = useContextStore((s) => s.pendingCandidateCount)
  // Hide the Getting Started entry only on a confirmed dismissal. While the
  // preferences request is still in flight this is false, so the entry stays
  // visible — a brief flash for dismissed users beats a blank nav slot.
  const gettingStartedDismissed = useSettingsStore(
    (s) => s.preferences?.gettingStartedDismissed === true
  )

  const handleLogout = useCallback(() => {
    authManager.logout()
    void navigate({ to: '/login', search: {} })
  }, [authManager, navigate])

  function renderItem(item: NavItem) {
    const active = activePath.startsWith(item.path)
    const label = t(item.labelKey)
    const accessibleLabel = `${label}: ${item.description}`
    const Icon = item.Icon
    const badgeCount = item.id === 'context' ? pendingContextCount : 0
    return (
      <button
        key={item.id}
        data-testid={`sidebar-nav-${item.id}`}
        onClick={() => onNavigate(item.path)}
        aria-label={accessibleLabel}
        aria-current={active ? 'page' : undefined}
        className={cn(
          'relative flex items-center gap-2.5 rounded-lg transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          active
            ? 'bg-apple-blue/10 text-apple-blue shadow-[inset_0_0_0_1px_rgba(0,102,204,0.24)]'
            : 'text-foreground-light/80 dark:text-foreground-dark/80 hover:bg-black/[0.04] dark:hover:bg-white/[0.06] hover:text-foreground-light dark:hover:text-foreground-dark'
        )}
        title={accessibleLabel}
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
    const items = NAV_ITEMS.filter((item) => {
      if (item.id === 'context' && !contextGovernanceEnabled) return false
      if (item.id === 'start' && gettingStartedDismissed) return false
      return true
    })
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
      {isAdmin &&
        renderItem({
          id: 'admin',
          Icon: Shield,
          labelKey: 'nav.admin',
          description: 'manage organizations, users, and system health',
          path: '/admin',
        })}
      <button
        data-testid="sidebar-nav-logout"
        onClick={handleLogout}
        aria-label="Logout: sign out of this workspace"
        className={cn(
          'flex items-center gap-2.5 rounded-lg transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          'text-foreground-light/80 dark:text-foreground-dark/80 hover:bg-red-500/10 hover:text-red-500'
        )}
        title="Logout: sign out of this workspace"
      >
        <LogOut size={16} strokeWidth={2} className="flex-shrink-0" />
        {expanded && <span className="truncate text-ui-body font-medium">{t('nav.logout')}</span>}
      </button>
    </div>
  )
}
