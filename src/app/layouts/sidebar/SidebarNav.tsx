import { useCallback, useMemo } from 'react'
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
  Moon,
  Sun,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { useAuth } from '@app/shared/model/auth.context'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { useContextStore } from '@app/features/context'
import { isApiAgent, useAgentsStore } from '@app/entities/agent'
import { useNavigationStore } from '@app/entities/navigation'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { useSettingsStore } from '@app/entities/settings'
import { useSkillsStore } from '@app/entities/skill'
import { shouldShowGettingStarted } from '@app/shared/lib/gettingStartedPreference'
import {
  getGettingStartedProgress,
  summarizeGettingStartedTasks,
} from '@app/shared/lib/gettingStartedProgress'
import { useTheme } from '@app/shared/model/theme.context'

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
    description: 'see tasks and check progress',
    path: '/tasks',
  },
  {
    id: 'inbox',
    Icon: Inbox,
    labelKey: 'nav.inbox',
    description: 'check updates that need a next step',
    path: '/inbox',
  },
  {
    id: 'context',
    Icon: ClipboardCheck,
    labelKey: 'nav.context',
    description: 'check saved notes and guidance',
    path: '/context',
  },
  {
    id: 'agents',
    Icon: Bot,
    labelKey: 'nav.agents',
    description: 'create and manage agents',
    path: '/agents',
  },
  {
    id: 'skills',
    Icon: Zap,
    labelKey: 'nav.skills',
    description: 'reuse guidance',
    path: '/skills',
  },
  {
    id: 'analytics',
    Icon: BarChart3,
    labelKey: 'nav.analytics',
    description: 'see agent activity and results',
    path: '/analytics',
  },
]

const BOTTOM_ITEMS: NavItem[] = [
  {
    id: 'billing',
    Icon: CreditCard,
    labelKey: 'nav.billing',
    description: 'check plan, payments, and invoices',
    path: '/billing',
  },
  {
    id: 'settings',
    Icon: Settings,
    labelKey: 'nav.settings',
    description: 'manage teams, agents, and access',
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
  const { theme, toggleTheme } = useTheme()
  const themeLabel =
    theme === 'dark' ? t('appLayout.topBar.switchToLight') : t('appLayout.topBar.switchToDark')
  // Mirror the backend platform-admin gate (`AdminService::require_platform_admin`,
  // #881) and the /admin route guard: only a platform admin (`users.is_admin`)
  // reaches the admin console. The flag is hydrated from `/me` (it is not in the
  // JWT); gating on the self-assignable per-org `role` would surface the link to
  // every org owner.
  const isAdmin = user?.isAdmin === true
  const contextGovernanceEnabled = useContextFeaturesStore((s) => s.governance)
  const pendingContextCount = useContextStore((s) => s.pendingCandidateCount)
  // Start is beginner-default. Only an explicit skip/completion preference
  // hides it from the sidebar.
  const showGettingStarted = useSettingsStore((s) => shouldShowGettingStarted(s.preferences))
  const teams = useNavigationStore((s) => s.teams)
  const projectsByTeam = useNavigationStore((s) => s.projects)
  const agentGroups = useNavigationStore((s) => s.agentGroups)
  const agents = useAgentsStore((s) => s.agents)
  const providers = useSettingsStore((s) => s.providers)
  const runtimeSettings = useSettingsStore((s) => s.runtimeSettings)
  const skills = useSkillsStore((s) => s.skills)
  const boardColumns = useBoardStore((s) => s.columns)
  const selectedGroupId = useBoardStore((s) => s.selectedGroupId)
  const projects = useMemo(() => Object.values(projectsByTeam).flat(), [projectsByTeam])
  const tasks = useMemo(() => Object.values(boardColumns).flat(), [boardColumns])
  const taskSnapshot = useMemo(() => summarizeGettingStartedTasks(tasks), [tasks])
  const checklistProgress = getGettingStartedProgress({
    hasWorkspace: teams.length > 0 && projects.length > 0,
    runtimeReady: Boolean(
      runtimeSettings &&
      runtimeSettings.availableRuntimes.length > 0 &&
      runtimeSettings.availableCliTools.length > 0
    ),
    executionCredentialReady:
      providers.some((provider) => provider.isEnabled && provider.lastTestStatus === 'passed') ||
      agents.some((agent) => agent.cliTool),
    hasAgent: agents.some((agent) => !isApiAgent(agent)),
    hasRouting: Boolean(selectedGroupId ?? agentGroups[0]?.id),
    taskSnapshot,
    hasReusableLearning: skills.length > 0 || taskSnapshot.appliedSkills > 0,
  })

  const handleLogout = useCallback(() => {
    authManager.logout()
    void navigate({ to: '/login', search: {} })
  }, [authManager, navigate])

  function renderItem(item: NavItem) {
    const active = activePath.startsWith(item.path)
    const label = t(item.labelKey)
    const Icon = item.Icon
    const badgeCount = item.id === 'context' ? pendingContextCount : 0
    const showChecklistBadge =
      item.id === 'start' && checklistProgress.completeCount < checklistProgress.total
    const checklistLabel = `${checklistProgress.completeCount}/${checklistProgress.total}`
    const accessibleLabel = `${label}: ${item.description}${showChecklistBadge ? `. ${checklistLabel}` : ''}`
    return (
      <button
        key={item.id}
        data-testid={`sidebar-nav-${item.id}`}
        onClick={() => onNavigate(item.path)}
        aria-label={accessibleLabel}
        aria-current={active ? 'page' : undefined}
        className={cn(
          'relative flex items-center gap-2.5 rounded-button text-ui-body transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          active
            ? 'rounded-button bg-black/[0.06] text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark'
            : 'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
        )}
        title={accessibleLabel}
      >
        <Icon size={16} strokeWidth={2} className="flex-shrink-0" />
        {expanded && <span className="truncate font-medium">{label}</span>}
        {badgeCount > 0 && (
          <span
            data-testid="context-approval-nav-badge"
            className={cn(
              'h-5 min-w-5 rounded-button bg-black/[0.05] px-1.5 text-center text-ui-caption font-medium leading-5 text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark',
              expanded ? 'ml-auto' : 'absolute -right-1 -top-1'
            )}
          >
            {badgeCount > 99 ? '99+' : badgeCount}
          </span>
        )}
        {showChecklistBadge && (
          <span
            data-testid="setup-checklist-nav-badge"
            className={cn(
              'h-5 min-w-5 rounded-button bg-black/[0.05] px-1.5 text-center text-ui-caption font-medium leading-5 tabular-nums text-secondary-light dark:bg-white/[0.08] dark:text-secondary-dark',
              expanded ? 'ml-auto' : 'absolute -right-1 -top-1'
            )}
          >
            {checklistLabel}
          </span>
        )}
      </button>
    )
  }

  if (section === 'primary') {
    const items = NAV_ITEMS.filter((item) => {
      if (item.id === 'context' && !contextGovernanceEnabled) return false
      if (item.id === 'start' && !showGettingStarted) return false
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
          description: 'manage team spaces, people, and app health',
          path: '/admin',
        })}
      <button
        type="button"
        onClick={toggleTheme}
        aria-label={themeLabel}
        className={cn(
          'flex items-center gap-2.5 rounded-button text-ui-body transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          'text-secondary-light hover:bg-black/[0.04] hover:text-foreground-light dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark'
        )}
        title={themeLabel}
      >
        {theme === 'dark' ? (
          <Sun size={16} strokeWidth={2} className="flex-shrink-0" aria-hidden="true" />
        ) : (
          <Moon size={16} strokeWidth={2} className="flex-shrink-0" aria-hidden="true" />
        )}
        {expanded && <span className="truncate font-medium">{t('settings.theme')}</span>}
      </button>
      <button
        data-testid="sidebar-nav-logout"
        onClick={handleLogout}
        aria-label="Logout: sign out of Forge"
        className={cn(
          'flex items-center gap-2.5 rounded-button text-ui-body transition-colors',
          expanded ? 'px-2.5 py-1.5 w-full' : 'w-9 h-9 justify-center',
          'text-foreground-light/80 dark:text-foreground-dark/80 hover:bg-red-500/10 hover:text-red-500'
        )}
        title="Logout: sign out of Forge"
      >
        <LogOut size={16} strokeWidth={2} className="flex-shrink-0" />
        {expanded && <span className="truncate font-medium">{t('nav.logout')}</span>}
      </button>
    </div>
  )
}
