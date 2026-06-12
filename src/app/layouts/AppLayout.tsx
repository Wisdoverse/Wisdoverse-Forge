import { useState, useCallback, useEffect, useMemo, type ReactNode } from 'react'
import { PanelRightOpen } from 'lucide-react'
import { useRouterState } from '@tanstack/react-router'
import { Sidebar } from './sidebar'
import { TopBar } from './TopBar'
import { RightPanel } from './RightPanel'
import { useBoardStore } from '@app/shared/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'
import { useTheme } from '@app/shared/model/theme.context'
import { CommandPalette } from '@app/features/cmdk/CommandPalette'
import { TaskFormModal, type TaskProjectOption } from '@app/features/board/TaskFormModal'
import { AgentGroupSelector } from '@app/features/board/AgentGroupSelector'
import { orchestrationApi, type ParticipantSummary } from '@app/shared/api/orchestration'

interface AppLayoutProps {
  children: ReactNode
  activePath?: string
  onNavigate?: (path: string) => void
}

const PAGE_META: Record<string, { title: string; subtitle?: string }> = {
  '/start': { title: 'Start', subtitle: 'First-run setup and launch checklist' },
  '/tasks': { title: 'Tasks', subtitle: 'Plan, assign, and track agent work' },
  '/inbox': { title: 'Inbox', subtitle: 'Notifications and updates' },
  '/context/audit': { title: 'Review history', subtitle: 'See past reuse decisions and exports' },
  '/context': {
    title: 'Saved memories and instructions',
    subtitle: 'Review what agents may reuse later',
  },
  '/agents': { title: 'Agents', subtitle: 'Create and manage agents that handle tasks' },
  '/skills': { title: 'Saved instructions', subtitle: 'Instructions agents can follow again' },
  '/analytics': { title: 'Analytics', subtitle: 'See agent activity and results' },
  '/billing': { title: 'Billing', subtitle: 'Plan, usage, and invoices' },
  '/settings': { title: 'Settings', subtitle: 'Account, AI services, and workspace' },
  '/admin': { title: 'Admin', subtitle: 'System health and user management' },
}

function resolvePageMeta(path: string): { title: string; subtitle?: string } {
  const match = Object.entries(PAGE_META).find(([key]) => path.startsWith(key))
  return match ? match[1] : { title: 'Wisdoverse Forge' }
}

export function AppLayout({
  children,
  activePath: _propPath = '/tasks',
  onNavigate,
}: AppLayoutProps) {
  const activePath = useRouterState({ select: (s) => s.location.pathname })
  const { viewMode, groupBy, setViewMode, setGroupBy } = useBoardStore()
  const toggleSidebar = useNavigationStore((s) => s.toggleSidebar)
  const sidebarExpanded = useNavigationStore((s) => s.sidebarExpanded)
  const { toggleTheme } = useTheme()
  const [panelCollapsed, setPanelCollapsed] = useState(true)
  const [isMobile, setIsMobile] = useState(() =>
    typeof window !== 'undefined' ? window.innerWidth < 768 : false
  )

  // Auto-collapse sidebar on narrow viewports
  useEffect(() => {
    function handleResize() {
      const mobile = window.innerWidth < 768
      setIsMobile(mobile)
      if (mobile && useNavigationStore.getState().sidebarExpanded) {
        useNavigationStore.setState({ sidebarExpanded: false })
      }
    }
    handleResize()
    window.addEventListener('resize', handleResize)
    return () => window.removeEventListener('resize', handleResize)
  }, [])
  const [cmdkOpen, setCmdkOpen] = useState(false)
  const [taskFormOpen, setTaskFormOpen] = useState(false)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const selectedTaskId = useBoardStore((s) => s.selectedTaskId)
  const selectedGroupId = useBoardStore((s) => s.selectedGroupId)
  const setSelectedGroupId = useBoardStore((s) => s.setSelectedGroupId)
  const setSelectedTask = useBoardStore((s) => s.setSelectedTask)
  const upsertTask = useBoardStore((s) => s.upsertTask)
  const navTeams = useNavigationStore((s) => s.teams)
  const navProjects = useNavigationStore((s) => s.projects)
  const selectedProjectId = useNavigationStore((s) => s.selectedProjectId)
  const selectProject = useNavigationStore((s) => s.selectProject)
  const agentGroups = useNavigationStore((s) => s.agentGroups)
  const selectedTaskGroup = useMemo(
    () => agentGroups.find((group) => group.id === selectedGroupId) ?? null,
    [agentGroups, selectedGroupId]
  )

  const taskProjectOptions = useMemo<TaskProjectOption[]>(
    () =>
      navTeams.flatMap((team) =>
        (navProjects[team.id] ?? []).map((project) => ({
          id: project.id,
          name: project.name,
          teamId: team.id,
          teamName: team.name,
          color: project.color,
        }))
      ),
    [navTeams, navProjects]
  )

  // Auto-open the right panel when a task is selected — otherwise clicking a
  // task card appears to do nothing (the detail panel only renders when the
  // panel is expanded, and it defaults to collapsed on first load).
  useEffect(() => {
    if (selectedTaskId) setPanelCollapsed(false)
  }, [selectedTaskId])

  // Refresh the participant list every time the modal opens so a newly
  // registered agent shows up without a page reload. Failures are ignored —
  // the dropdown then renders the "no online agents, will queue" hint.
  useEffect(() => {
    if (!taskFormOpen) return
    let cancelled = false
    orchestrationApi
      .getParticipants('all')
      .then((list) => {
        if (!cancelled) setParticipants(list)
      })
      .catch(() => {
        if (!cancelled) setParticipants([])
      })
    return () => {
      cancelled = true
    }
  }, [taskFormOpen])
  const pageMeta = resolvePageMeta(activePath)
  const isTasksPage = activePath.startsWith('/tasks')

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        setCmdkOpen((prev) => !prev)
      }
      if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
        e.preventDefault()
        toggleSidebar()
      }
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [toggleSidebar])

  const handleNavigate = useCallback(
    (path: string) => {
      onNavigate?.(path)
      if (isMobile) {
        useNavigationStore.setState({ sidebarExpanded: false })
      }
    },
    [onNavigate, isMobile]
  )

  const handleCreateTaskForProject = useCallback(
    async (projectId: string) => {
      await selectProject(projectId)
      handleNavigate('/tasks')
      setTaskFormOpen(true)
    },
    [handleNavigate, selectProject]
  )

  function handleCommandSelect(commandId: string) {
    if (commandId.startsWith('nav:')) {
      const path = `/${commandId.replace('nav:', '')}`
      handleNavigate(path)
    } else if (commandId.startsWith('view:')) {
      const view = commandId.replace('view:', '') as typeof viewMode
      setViewMode(view)
    } else if (commandId === 'action:toggle-theme') {
      toggleTheme()
    }
  }

  // On mobile, sidebar is hidden unless expanded (where it overlays content)
  const showSidebar = !isMobile || sidebarExpanded
  const sidebarAsOverlay = isMobile && sidebarExpanded
  const closeMobileDetail = () => {
    setSelectedTask(null)
    setPanelCollapsed(true)
  }

  return (
    <div className="flex h-[100dvh] md:h-screen gap-2 p-2 overflow-hidden bg-background-light dark:bg-background-dark relative">
      {sidebarAsOverlay && (
        <button
          type="button"
          aria-label="Close sidebar"
          onClick={() => useNavigationStore.setState({ sidebarExpanded: false })}
          className="absolute inset-0 z-20 bg-black/30 backdrop-blur-sm"
        />
      )}
      {showSidebar && (
        <div className={sidebarAsOverlay ? 'absolute z-30 top-2 bottom-2 left-2' : 'contents'}>
          <Sidebar
            activePath={activePath}
            onNavigate={handleNavigate}
            onCreateTaskForProject={handleCreateTaskForProject}
          />
        </div>
      )}
      <div className="flex flex-col flex-1 gap-2 min-w-0">
        <TopBar
          title={pageMeta.title}
          subtitle={pageMeta.subtitle}
          showTaskControls={isTasksPage}
          onMenuClick={
            isMobile ? () => useNavigationStore.setState({ sidebarExpanded: true }) : undefined
          }
          viewMode={viewMode}
          groupBy={groupBy}
          onViewChange={setViewMode}
          onGroupByChange={setGroupBy}
          onCreateTask={() => setTaskFormOpen(true)}
          agentGroupSelector={
            isTasksPage ? (
              <AgentGroupSelector
                groups={agentGroups}
                selectedGroupId={selectedGroupId}
                selectedProjectId={selectedProjectId}
                onSelectGroup={setSelectedGroupId}
              />
            ) : undefined
          }
          onCmdK={() => setCmdkOpen(true)}
        />
        <main data-testid="main-content" className="flex-1 overflow-auto rounded-panel">
          {children}
        </main>
      </div>
      {!isMobile && panelCollapsed ? (
        <button
          type="button"
          data-testid="activity-panel-toggle"
          onClick={() => setPanelCollapsed(false)}
          aria-label="Show activity panel"
          className="mt-2 flex h-9 items-center gap-2 self-start whitespace-nowrap rounded-full border border-black/[0.08] bg-white px-3 text-ui-caption font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light dark:border-white/[0.1] dark:bg-surface-dark dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
          title="Show activity panel"
        >
          <PanelRightOpen size={15} strokeWidth={2} aria-hidden="true" />
          <span>Activity</span>
        </button>
      ) : !isMobile ? (
        <RightPanel
          collapsed={panelCollapsed}
          onToggle={() => setPanelCollapsed(!panelCollapsed)}
        />
      ) : null}
      {isMobile && selectedTaskId && !panelCollapsed && (
        <>
          <button
            type="button"
            aria-label="Close task detail"
            onClick={closeMobileDetail}
            className="absolute inset-0 z-30 bg-black/35 backdrop-blur-sm"
          />
          <div className="absolute inset-x-2 top-[68px] bottom-2 z-40">
            <RightPanel collapsed={false} onToggle={closeMobileDetail} variant="mobile" />
          </div>
        </>
      )}
      <CommandPalette
        isOpen={cmdkOpen}
        onClose={() => setCmdkOpen(false)}
        onSelect={handleCommandSelect}
      />
      <TaskFormModal
        isOpen={taskFormOpen}
        onClose={() => setTaskFormOpen(false)}
        agents={participants.map((p) => ({ id: p.agentId, name: p.name, status: p.status }))}
        projects={taskProjectOptions}
        selectedProjectId={selectedProjectId}
        selectedTaskGroupId={selectedGroupId}
        selectedTaskGroupName={selectedTaskGroup?.name ?? null}
        onProjectChange={selectProject}
        onOpenTaskRouting={() => {
          setTaskFormOpen(false)
          handleNavigate('/agents')
        }}
        onSubmit={async (data) => {
          if (!data.projectId) {
            throw new Error('Choose a project before creating a task.')
          }
          let groupId = useBoardStore.getState().selectedGroupId
          if (useNavigationStore.getState().selectedProjectId !== data.projectId || !groupId) {
            await selectProject(data.projectId)
            groupId = useBoardStore.getState().selectedGroupId
          }
          if (!groupId) {
            throw new Error(
              'Create a task queue before creating a task. Agents check task queues for new tasks. Open Agents, then Task Queues, and create one.'
            )
          }
          const response = await orchestrationApi.createTask({
            groupId,
            params: { task: data.title, message: data.description || data.title },
            priority: data.priority,
            // Empty string from the dropdown means "leave it to auto-dispatch".
            ...(data.assignedTo ? { assignedTo: data.assignedTo } : {}),
          })
          if (response.ok && response.task) {
            upsertTask(response.task)
            return
          }
          throw response
        }}
      />
    </div>
  )
}
