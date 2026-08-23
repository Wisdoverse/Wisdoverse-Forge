import { useState, useCallback, useEffect, useMemo, type ReactNode } from 'react'
import { PanelRightOpen } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useRouterState } from '@tanstack/react-router'
import { Sidebar } from './sidebar'
import { TopBar } from './TopBar'
import { RightPanel } from './RightPanel'
import { useBoardStore } from '@app/entities/navigation/model/board.store'
import { useNavigationStore } from '@app/entities/navigation'
import { useTheme } from '@app/shared/model/theme.context'
import { useSettingsStore } from '@app/entities/settings'
import { CommandPalette } from '@app/features/cmdk'
import { TaskFormModal, type TaskProjectOption } from '@app/features/board'
import { orchestrationApi, type ParticipantSummary } from '@app/shared/api/orchestration'

interface AppLayoutProps {
  children: ReactNode
  activePath?: string
  onNavigate?: (path: string) => void
}

const PAGE_META = [
  {
    path: '/start',
    titleKey: 'appLayout.pages.start.title',
    subtitleKey: 'appLayout.pages.start.subtitle',
  },
  {
    path: '/tasks',
    titleKey: 'appLayout.pages.tasks.title',
    subtitleKey: 'appLayout.pages.tasks.subtitle',
  },
  {
    path: '/inbox',
    titleKey: 'appLayout.pages.inbox.title',
    subtitleKey: 'appLayout.pages.inbox.subtitle',
  },
  {
    path: '/context/audit',
    titleKey: 'appLayout.pages.savedItemHistory.title',
    subtitleKey: 'appLayout.pages.savedItemHistory.subtitle',
  },
  {
    path: '/context',
    titleKey: 'appLayout.pages.savedItems.title',
    subtitleKey: 'appLayout.pages.savedItems.subtitle',
  },
  {
    path: '/agents',
    titleKey: 'appLayout.pages.agents.title',
    subtitleKey: 'appLayout.pages.agents.subtitle',
  },
  {
    path: '/skills',
    titleKey: 'appLayout.pages.skills.title',
    subtitleKey: 'appLayout.pages.skills.subtitle',
  },
  {
    path: '/analytics',
    titleKey: 'appLayout.pages.analytics.title',
    subtitleKey: 'appLayout.pages.analytics.subtitle',
  },
  {
    path: '/billing',
    titleKey: 'appLayout.pages.billing.title',
    subtitleKey: 'appLayout.pages.billing.subtitle',
  },
  {
    path: '/settings',
    titleKey: 'appLayout.pages.settings.title',
    subtitleKey: 'appLayout.pages.settings.subtitle',
  },
  {
    path: '/admin',
    titleKey: 'appLayout.pages.admin.title',
    subtitleKey: 'appLayout.pages.admin.subtitle',
  },
] as const

function resolvePageMeta(path: string): { titleKey: string; subtitleKey?: string } {
  const match = PAGE_META.find((meta) => path.startsWith(meta.path))
  return match ?? { titleKey: 'appLayout.pages.fallback.title' }
}

export function AppLayout({
  children,
  activePath: _propPath = '/tasks',
  onNavigate,
}: AppLayoutProps) {
  const { t } = useTranslation()
  const activePath = useRouterState({ select: (s) => s.location.pathname })
  const { viewMode, setViewMode } = useBoardStore()
  const toggleSidebar = useNavigationStore((s) => s.toggleSidebar)
  const sidebarExpanded = useNavigationStore((s) => s.sidebarExpanded)
  const setGettingStartedDismissed = useSettingsStore((s) => s.setGettingStartedDismissed)
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
  const [taskCreatedMessage, setTaskCreatedMessage] = useState<string | null>(null)
  const [participants, setParticipants] = useState<ParticipantSummary[]>([])
  const selectedGroupId = useBoardStore((s) => s.selectedGroupId)
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

  useEffect(() => {
    if (!activePath.startsWith('/tasks')) setTaskCreatedMessage(null)
  }, [activePath])

  // Refresh the participant list every time the modal opens so a newly
  // registered agent shows up without a page reload. Failures are ignored —
  // the dropdown then renders the "no online agents" hint.
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
  const hasProjectOptions = taskProjectOptions.length > 0
  const createTaskSetup =
    !selectedProjectId && !hasProjectOptions
      ? {
          label: t('commandPalette.taskSetup.noProjectOptions.label'),
          buttonLabel: t('commandPalette.taskSetup.noProjectOptions.buttonLabel'),
          description: t('commandPalette.taskSetup.noProjectOptions.description'),
          searchText:
            'new task create task first task project setup 创建任务 新任务 第一个任务 项目 设置',
        }
      : !selectedProjectId
        ? {
            label: t('commandPalette.taskSetup.chooseProject.label'),
            buttonLabel: t('commandPalette.taskSetup.chooseProject.buttonLabel'),
            description: t('commandPalette.taskSetup.chooseProject.description'),
            searchText: 'new task create task choose project send work 创建任务 新任务 选择项目',
          }
        : !selectedGroupId
          ? {
              label: t('commandPalette.taskSetup.noWaitingPlace.label'),
              buttonLabel: t('commandPalette.taskSetup.noWaitingPlace.buttonLabel'),
              description: t('commandPalette.taskSetup.noWaitingPlace.description'),
              searchText:
                'new task create task first task agent place setup 创建任务 新任务 智能体 任务位置 设置',
            }
          : {
              label: t('commandPalette.taskSetup.ready.label'),
              buttonLabel: t('commandPalette.taskSetup.ready.buttonLabel'),
              description: t('commandPalette.taskSetup.ready.description'),
              searchText: 'new task create task send work 创建任务 新任务 任务 工作',
            }
  const createTaskCommand = {
    label: createTaskSetup.label,
    description: createTaskSetup.description,
    searchText: createTaskSetup.searchText,
  }
  const pageTitle = t(pageMeta.titleKey)
  const pageSubtitle = pageMeta.subtitleKey ? t(pageMeta.subtitleKey) : undefined

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
      setTaskCreatedMessage(null)
      if (!(await selectProject(projectId))) {
        // One retry: the common cause is a transient fetch failure, and the
        // task form would otherwise claim the project has nowhere tasks can wait.
        await selectProject(projectId)
      }
      if (!useBoardStore.getState().selectedGroupId) {
        handleNavigate('/agents')
        return
      }
      handleNavigate('/tasks')
      setTaskFormOpen(true)
    },
    [handleNavigate, selectProject]
  )

  const handleNewTaskAction = useCallback(() => {
    setTaskCreatedMessage(null)
    if (!selectedProjectId && !hasProjectOptions) {
      handleNavigate('/settings/projects')
      return
    }
    if (selectedProjectId && !selectedGroupId) {
      handleNavigate('/agents')
      return
    }
    handleNavigate('/tasks')
    setTaskFormOpen(true)
  }, [handleNavigate, hasProjectOptions, selectedGroupId, selectedProjectId])

  function handleCommandSelect(commandId: string) {
    if (commandId.startsWith('nav:')) {
      const path = `/${commandId.replace('nav:', '')}`
      handleNavigate(path)
    } else if (commandId.startsWith('view:')) {
      const view = commandId.replace('view:', '') as typeof viewMode
      handleNavigate('/tasks')
      setViewMode(view)
    } else if (commandId.startsWith('settings:')) {
      const section = commandId.replace('settings:', '')
      handleNavigate(`/settings/${section}`)
    } else if (commandId === 'action:create-task') {
      handleNewTaskAction()
    } else if (commandId === 'action:work-tool-sign-ins') {
      handleNavigate('/settings/work-tool-sign-ins')
    } else if (commandId === 'action:show-setup-checklist') {
      void restoreSetupChecklistFromCommand()
    } else if (commandId === 'action:toggle-theme') {
      toggleTheme()
    }
  }

  async function restoreSetupChecklistFromCommand() {
    const restored = await setGettingStartedDismissed(false)
    handleNavigate(restored ? '/start' : '/settings/account')
  }

  // On mobile, sidebar is hidden unless expanded (where it overlays content)
  const showSidebar = !isMobile || sidebarExpanded
  const sidebarAsOverlay = isMobile && sidebarExpanded
  return (
    <div className="relative flex h-[100dvh] overflow-hidden bg-background-light dark:bg-background-dark md:h-screen">
      {sidebarAsOverlay && (
        <button
          type="button"
          aria-label="Close left menu"
          onClick={() => useNavigationStore.setState({ sidebarExpanded: false })}
          className="absolute inset-0 z-20 bg-black/30 backdrop-blur-sm"
        />
      )}
      {showSidebar && (
        <div className={sidebarAsOverlay ? 'absolute inset-y-0 left-0 z-30' : 'contents'}>
          <Sidebar
            activePath={activePath}
            onNavigate={handleNavigate}
            onCreateTaskForProject={handleCreateTaskForProject}
          />
        </div>
      )}
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar
          title={pageTitle}
          subtitle={pageSubtitle}
          showTaskControls={isTasksPage}
          onMenuClick={
            isMobile ? () => useNavigationStore.setState({ sidebarExpanded: true }) : undefined
          }
          viewMode={viewMode}
          onViewChange={setViewMode}
          onCreateTask={handleNewTaskAction}
          createTaskLabel={createTaskSetup.buttonLabel}
          createTaskTitle={createTaskSetup.description}
          onCmdK={() => setCmdkOpen(true)}
        />
        {taskCreatedMessage && (
          <div
            data-testid="task-created-status"
            role="status"
            aria-live="polite"
            className="flex items-center gap-2 rounded-card border border-black/[0.08] bg-black/[0.025] px-4 py-2 text-ui-caption font-medium text-secondary-light dark:border-white/[0.08] dark:bg-white/[0.03] dark:text-secondary-dark"
          >
            <span className="h-1.5 w-1.5 rounded-full bg-apple-blue" aria-hidden="true" />
            {taskCreatedMessage}
          </div>
        )}
        <main
          data-testid="main-content"
          className="flex-1 overflow-auto bg-white dark:bg-surface-dark"
        >
          {children}
        </main>
      </div>
      {!isMobile && panelCollapsed ? (
        <button
          type="button"
          data-testid="activity-panel-toggle"
          onClick={() => setPanelCollapsed(false)}
          aria-label="Show live task updates"
          className="m-2 flex h-8 items-center gap-2 self-start whitespace-nowrap rounded-button border border-black/[0.08] bg-white px-3 text-ui-button font-medium text-secondary-light transition-colors hover:bg-black/[0.04] hover:text-foreground-light dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-secondary-dark dark:hover:bg-white/[0.06] dark:hover:text-foreground-dark"
          title="Show live task updates"
        >
          <PanelRightOpen size={15} strokeWidth={2} aria-hidden="true" />
          <span>Activity</span>
        </button>
      ) : !isMobile ? (
        <RightPanel
          collapsed={panelCollapsed}
          onToggle={() => setPanelCollapsed(!panelCollapsed)}
          onOpenBoard={() => handleNavigate('/tasks')}
        />
      ) : null}
      <CommandPalette
        isOpen={cmdkOpen}
        onClose={() => setCmdkOpen(false)}
        onSelect={handleCommandSelect}
        createTaskCommand={createTaskCommand}
      />
      <TaskFormModal
        isOpen={taskFormOpen}
        onClose={() => setTaskFormOpen(false)}
        agents={participants.map((p) => ({
          id: p.agentId,
          name: p.name,
          status: p.status,
          capabilities: p.capabilities,
          runtimeKind: p.runtimeKind,
        }))}
        projects={taskProjectOptions}
        selectedProjectId={selectedProjectId}
        selectedTaskGroupId={selectedGroupId}
        selectedTaskGroupName={selectedTaskGroup?.name ?? null}
        onProjectChange={selectProject}
        onOpenAgentSetup={() => {
          setTaskFormOpen(false)
          handleNavigate('/agents')
        }}
        onOpenProjectSettings={() => {
          setTaskFormOpen(false)
          handleNavigate('/settings/projects')
        }}
        onOpenTaskRouting={() => {
          setTaskFormOpen(false)
          handleNavigate('/agents')
        }}
        onSubmit={async (data) => {
          setTaskCreatedMessage(null)
          if (!data.projectId) {
            throw new Error('Choose a project before creating a task.')
          }
          let groupId = useBoardStore.getState().selectedGroupId
          let lanesLoaded = true
          if (useNavigationStore.getState().selectedProjectId !== data.projectId || !groupId) {
            lanesLoaded = await selectProject(data.projectId)
            groupId = useBoardStore.getState().selectedGroupId
          }
          if (!groupId && !lanesLoaded) {
            throw new Error(
              'Forge could not load the place for new tasks in this project. Select the project again, then create the task.'
            )
          }
          if (!groupId) {
            throw new Error(
              'Set up a place for new tasks before creating a task. This gives new work somewhere to wait before an agent starts it. Open Agents to set it up, then come back here.'
            )
          }
          const response = await orchestrationApi.createTask({
            groupId,
            params: {
              task: data.title,
              message: data.description || data.title,
              ...(data.imageAttachmentIds && data.imageAttachmentIds.length > 0
                ? { imageAttachmentIds: data.imageAttachmentIds }
                : {}),
            },
            priority: data.priority,
            // Empty string from the dropdown means "leave it to auto-dispatch".
            ...(data.assignedTo ? { assignedTo: data.assignedTo } : {}),
          })
          if (response.ok && response.task) {
            let task = response.task
            if (!data.assignedTo) {
              try {
                const startResponse = await orchestrationApi.updateTask(task.id, {
                  state: 'queued',
                })
                if (!startResponse.ok || !startResponse.task) throw startResponse
                task = startResponse.task
              } catch {
                upsertTask(task)
                setTaskCreatedMessage(
                  'Task saved but not started. Move it to Waiting to start to retry.'
                )
                return
              }
            }
            upsertTask(task)
            setTaskCreatedMessage(
              'Task saved on the board. Watch it there for progress, then open it when it is ready to check.'
            )
            return
          }
          throw response
        }}
      />
    </div>
  )
}
