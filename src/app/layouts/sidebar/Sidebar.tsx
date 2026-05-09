import { cn } from '@app/shared/lib/utils'
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
        'flex flex-shrink-0 flex-col overflow-hidden py-2 transition-all duration-300 ease-out',
        'rounded-panel border border-black/[0.08] bg-white dark:border-white/[0.1] dark:bg-surface-dark',
        sidebarExpanded ? 'w-[240px]' : 'w-[52px]'
      )}
    >
      <SidebarHeader expanded={sidebarExpanded} onToggle={toggleSidebar} />

      {sidebarExpanded && (
        <OrgSwitcher orgs={orgs} selectedOrgId={selectedOrgId} onSelect={selectOrg} />
      )}

      <div className={sidebarExpanded ? 'mt-1' : 'mt-2'}>
        <SidebarNav
          expanded={sidebarExpanded}
          activePath={activePath}
          onNavigate={onNavigate}
          section="primary"
        />
      </div>

      {sidebarExpanded ? (
        <>
          <div className="h-px bg-black/[0.06] dark:bg-white/[0.06] mx-3 my-2" />

          <div className="px-4 pb-1 text-ui-caption font-medium uppercase text-secondary-light dark:text-secondary-dark">
            Projects
          </div>

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

          <div className="h-px bg-black/[0.06] dark:bg-white/[0.06] mx-3 my-1" />
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
