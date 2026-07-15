import { useTranslation } from 'react-i18next'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useNavigationStore } from '@app/entities/navigation'
import { SidebarHeader } from './SidebarHeader'
import { OrgSwitcher } from './OrgSwitcher'
import { ProjectTree } from './ProjectTree'
import { SidebarNav } from './SidebarNav'

interface SidebarProps {
  activePath: string
  onNavigate: (path: string) => void
  onCreateTaskForProject?: (projectId: string) => void | Promise<void>
}

export function Sidebar({ activePath, onNavigate, onCreateTaskForProject }: SidebarProps) {
  const { t } = useTranslation()
  const {
    orgs,
    selectedOrgId,
    teams,
    projects,
    expandedTeams,
    selectedProjectId,
    sidebarExpanded,
    toggleSidebar,
    selectOrg,
    selectProject,
    updateTeam,
    deleteTeam,
    updateProject,
    deleteProject,
    toggleTeam,
  } = useNavigationStore()

  return (
    <nav
      data-testid="sidebar"
      className={cn(
        'flex flex-shrink-0 flex-col overflow-hidden border-r border-black/[0.08] bg-background-light py-2 transition-all duration-300 ease-out dark:border-white/[0.1] dark:bg-background-dark',
        sidebarExpanded ? 'w-[240px]' : 'w-[52px]'
      )}
    >
      <SidebarHeader expanded={sidebarExpanded} onToggle={toggleSidebar} />

      {sidebarExpanded && (
        <OrgSwitcher orgs={orgs} selectedOrgId={selectedOrgId} onSelect={selectOrg} />
      )}

      <div className={sidebarExpanded ? 'mt-1' : 'mt-2'}>
        {sidebarExpanded && (
          <p className={cn(uiStyles.groupLabel, 'px-4')}>{t('nav.groups.workspace')}</p>
        )}
        <SidebarNav
          expanded={sidebarExpanded}
          activePath={activePath}
          onNavigate={onNavigate}
          section="primary"
        />
      </div>

      {sidebarExpanded ? (
        <>
          <p className={cn(uiStyles.groupLabel, 'mt-4 px-4')}>{t('nav.groups.projects')}</p>

          <div className="flex-1 overflow-y-auto min-h-0 pb-2">
            <ProjectTree
              teams={teams}
              projects={projects}
              expandedTeams={expandedTeams}
              selectedProjectId={selectedProjectId}
              onToggleTeam={toggleTeam}
              onSelectProject={selectProject}
              onUpdateTeam={updateTeam}
              onDeleteTeam={deleteTeam}
              onUpdateProject={updateProject}
              onDeleteProject={deleteProject}
              onNavigate={onNavigate}
              onCreateTaskForProject={onCreateTaskForProject}
            />
          </div>

          <p className={cn(uiStyles.groupLabel, 'px-4 pt-2')}>{t('nav.groups.manage')}</p>
        </>
      ) : (
        <div className="flex-1" />
      )}

      <SidebarNav
        expanded={sidebarExpanded}
        activePath={activePath}
        onNavigate={onNavigate}
        section="secondary"
      />
    </nav>
  )
}
