import { useCallback, useEffect, useState, type FormEvent, type MouseEvent } from 'react'
import {
  Copy,
  FolderPlus,
  FolderOpen,
  Hash,
  ListPlus,
  Pencil,
  PlusCircle,
  Settings,
  Trash2,
  Users,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { ResourceMembersModal } from '@app/features/manage-members'
import type { NavProject } from '@app/entities/project'
import { projectApi } from '@app/entities/project'
import type { NavTeam } from '@app/entities/team'
import { userApi } from '@app/entities/user'

interface ProjectTreeProps {
  teams: NavTeam[]
  projects: Record<string, NavProject[]>
  expandedTeams: string[]
  selectedProjectId: string | null
  onToggleTeam: (teamId: string) => void
  onSelectProject: (projectId: string) => void | Promise<void>
  onUpdateTeam: (teamId: string, input: { name?: string }) => Promise<void>
  onDeleteTeam: (teamId: string) => Promise<void>
  onUpdateProject: (projectId: string, input: { name?: string }) => Promise<void>
  onDeleteProject: (projectId: string) => Promise<void>
  onNavigate?: (path: string) => void
  onCreateTaskForProject?: (projectId: string) => void | Promise<void>
}

interface ContextMenuPosition {
  x: number
  y: number
}

interface TeamMenuState extends ContextMenuPosition {
  team: NavTeam
}

interface ProjectMenuState {
  project: NavProject
  team: NavTeam
  x: number
  y: number
}

interface TeamEditorState {
  team: NavTeam
  name: string
  saving: boolean
  error: string | null
}

interface ProjectEditorState {
  project: NavProject
  name: string
  saving: boolean
  error: string | null
}

interface ProjectMenuItemProps {
  Icon: LucideIcon
  label: string
  detail?: string
  tone?: 'default' | 'primary' | 'danger'
  onClick: () => void
}

interface EmptyTreeHintProps {
  title: string
  detail: string
  actionLabel: string
  Icon: LucideIcon
  onAction?: () => void
  testId: string
}

const TEAM_MENU_SIZE = { width: 190, height: 108 }
const PROJECT_MENU_SIZE = { width: 280, height: 456 }

type RenameTarget = 'team' | 'project'

function parseApiStatus(error: unknown): { status: number | null; detail: string | null } {
  if (!(error instanceof Error)) return { status: null, detail: null }
  const message = error.message.trim()
  const match = /^API\s+(\d{3}):\s*(.*)$/s.exec(message)
  if (!match) return { status: null, detail: message || null }

  const status = Number(match[1])
  const body = match[2]?.trim()
  if (!body) return { status, detail: null }

  try {
    const parsed = JSON.parse(body) as unknown
    if (parsed && typeof parsed === 'object') {
      const data = parsed as Record<string, unknown>
      if (typeof data.error === 'string' && data.error.trim()) {
        return { status, detail: data.error.trim() }
      }
      if (typeof data.message === 'string' && data.message.trim()) {
        return { status, detail: data.message.trim() }
      }
    }
  } catch {
    // Preserve plain-text server details below.
  }

  return { status, detail: body }
}

function renameErrorMessage(target: RenameTarget, error: unknown): string {
  const label = target === 'team' ? 'team' : 'project'
  const title = target === 'team' ? 'Team' : 'Project'

  if (
    error instanceof TypeError ||
    (error instanceof Error && /^Failed to fetch$/i.test(error.message.trim()))
  ) {
    return `${title} name could not be saved. Forge could not connect while saving it. Check your connection, then save again.`
  }

  const { status, detail } = parseApiStatus(error)

  if (!status) {
    return renameValidationMessage(target, detail)
  }

  if (status === 401) {
    return `Sign in again, then reopen the sidebar and save this ${label} name.`
  }
  if (status === 403) {
    return `You do not have permission to rename this ${label}. Ask an owner or admin to update your access.`
  }
  if (status === 404) {
    return `This ${label} could not be found. Refresh the sidebar, then choose the current ${label} again.`
  }
  if (status === 409) {
    return `This ${label} changed while you were editing. Refresh the sidebar, review the current name, then try again.`
  }
  if (status === 422) {
    return renameValidationMessage(target, detail)
  }
  if (status === 429) {
    return `The sidebar is busy. Wait a moment, then save this ${label} name again.`
  }
  if (status >= 500) {
    return `Forge could not save this ${label} name right now. Refresh the sidebar, then save again. If it still fails, ask an owner or admin to check workspace setup.`
  }

  return `${title} name could not be saved. Refresh the sidebar and try again.`
}

function renameValidationMessage(target: RenameTarget, detail: string | null): string {
  const label = target === 'team' ? 'team' : 'project'
  const title = target === 'team' ? 'Team' : 'Project'
  const normalized = detail?.toLowerCase() ?? ''

  if (normalized.includes('name')) {
    return `Enter a ${label} name, then save again.`
  }
  if (normalized.includes('duplicate') || normalized.includes('already')) {
    return `Choose a different ${label} name, refresh the sidebar, then save again.`
  }

  return `${title} name could not be saved. Refresh the sidebar and try again.`
}

function getMenuPosition(
  menu: ContextMenuPosition,
  size: { width: number; height: number } = TEAM_MENU_SIZE
): { left: number; top: number } {
  if (typeof window === 'undefined') {
    return { left: menu.x, top: menu.y }
  }
  return {
    left: Math.max(8, Math.min(menu.x, window.innerWidth - size.width - 8)),
    top: Math.max(8, Math.min(menu.y, window.innerHeight - size.height - 8)),
  }
}

function ProjectMenuItem({ Icon, label, detail, tone = 'default', onClick }: ProjectMenuItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      className={cn(
        'flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left transition-colors',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35',
        tone === 'danger'
          ? 'text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20'
          : tone === 'primary'
            ? 'text-apple-blue hover:bg-apple-blue/10 dark:hover:bg-apple-blue/15'
            : 'text-foreground-light hover:bg-black/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.06]'
      )}
      onClick={onClick}
    >
      <Icon size={14} strokeWidth={2} aria-hidden="true" className="shrink-0" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-ui-caption font-medium">{label}</span>
        {detail ? (
          <span className="block truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
            {detail}
          </span>
        ) : null}
      </span>
    </button>
  )
}

function EmptyTreeHint({ title, detail, actionLabel, Icon, onAction, testId }: EmptyTreeHintProps) {
  return (
    <div
      data-testid={testId}
      className={cn(
        'mx-2 my-1 rounded-lg border px-2.5 py-2',
        'border-black/[0.06] bg-black/[0.02] dark:border-white/[0.08] dark:bg-white/[0.04]'
      )}
    >
      <div className="flex min-w-0 items-start gap-2">
        <Icon
          size={15}
          strokeWidth={2}
          aria-hidden="true"
          className="mt-0.5 shrink-0 text-apple-blue"
        />
        <div className="min-w-0">
          <p className="text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
            {title}
          </p>
          <p className="mt-0.5 text-ui-caption leading-snug text-secondary-light dark:text-secondary-dark">
            {detail}
          </p>
        </div>
      </div>
      {onAction ? (
        <button
          type="button"
          className={cn(
            'mt-2 inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-ui-caption font-medium',
            'text-apple-blue hover:bg-apple-blue/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue/35'
          )}
          onClick={onAction}
        >
          <PlusCircle size={13} strokeWidth={2} aria-hidden="true" />
          {actionLabel}
        </button>
      ) : null}
    </div>
  )
}

function canManageTeam(team: NavTeam): boolean {
  return team.canManage !== false
}

function canDeleteTeam(team: NavTeam): boolean {
  return team.canDelete !== false
}

function canManageProject(project: NavProject): boolean {
  return project.canManage !== false
}

function canDeleteProject(project: NavProject): boolean {
  return project.canDelete !== false
}

export function ProjectTree({
  teams,
  projects,
  expandedTeams,
  selectedProjectId,
  onToggleTeam,
  onSelectProject,
  onUpdateTeam,
  onDeleteTeam,
  onUpdateProject,
  onDeleteProject,
  onNavigate,
  onCreateTaskForProject,
}: ProjectTreeProps) {
  const [teamMenu, setTeamMenu] = useState<TeamMenuState | null>(null)
  const [projectMenu, setProjectMenu] = useState<ProjectMenuState | null>(null)
  const [teamEditor, setTeamEditor] = useState<TeamEditorState | null>(null)
  const [projectEditor, setProjectEditor] = useState<ProjectEditorState | null>(null)
  const [membersProject, setMembersProject] = useState<NavProject | null>(null)
  const [copyMessage, setCopyMessage] = useState<string | null>(null)

  const loadOrgUsers = useCallback(() => userApi.getUsers(), [])

  const loadSelectedProjectMembers = useCallback(async () => {
    if (!membersProject) return []
    return projectApi.getMembers(membersProject.id)
  }, [membersProject])

  const addSelectedProjectMember = useCallback(
    async (input: Parameters<typeof projectApi.addMember>[1]) => {
      if (!membersProject) throw new Error('No project selected')
      return projectApi.addMember(membersProject.id, input)
    },
    [membersProject]
  )

  const updateSelectedProjectMember = useCallback(
    async (userId: string, input: Parameters<typeof projectApi.updateMember>[2]) => {
      if (!membersProject) throw new Error('No project selected')
      return projectApi.updateMember(membersProject.id, userId, input)
    },
    [membersProject]
  )

  const removeSelectedProjectMember = useCallback(
    async (userId: string) => {
      if (!membersProject) throw new Error('No project selected')
      return projectApi.removeMember(membersProject.id, userId)
    },
    [membersProject]
  )

  useEffect(() => {
    if (!teamMenu && !projectMenu && !teamEditor && !projectEditor) return

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setTeamMenu(null)
        setProjectMenu(null)
        setTeamEditor(null)
        setProjectEditor(null)
      }
    }

    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [teamMenu, projectMenu, teamEditor, projectEditor])

  useEffect(() => {
    if (!copyMessage) return
    const timeout = window.setTimeout(() => setCopyMessage(null), 1800)
    return () => window.clearTimeout(timeout)
  }, [copyMessage])

  if (teams.length === 0) {
    return (
      <EmptyTreeHint
        testId="project-tree-empty-teams"
        Icon={Users}
        title="Create a team first"
        detail="Teams group projects and people. Add one, then create a project inside it."
        actionLabel="Open Team Settings"
        onAction={onNavigate ? () => onNavigate('/settings/teams') : undefined}
      />
    )
  }

  function handleTeamContextMenu(event: MouseEvent<HTMLButtonElement>, team: NavTeam) {
    event.preventDefault()
    event.stopPropagation()
    setProjectMenu(null)
    if (!canManageTeam(team) && !canDeleteTeam(team)) {
      setTeamMenu(null)
      return
    }
    setTeamMenu({ team, x: event.clientX, y: event.clientY })
  }

  function handleProjectContextMenu(
    event: MouseEvent<HTMLButtonElement>,
    team: NavTeam,
    project: NavProject
  ) {
    event.preventDefault()
    event.stopPropagation()
    setTeamMenu(null)
    setProjectMenu({ project, team, x: event.clientX, y: event.clientY })
  }

  function openTeamEditor(team: NavTeam) {
    setTeamMenu(null)
    setTeamEditor({ team, name: team.name, saving: false, error: null })
  }

  function openProjectEditor(project: NavProject) {
    setProjectMenu(null)
    setProjectEditor({ project, name: project.name, saving: false, error: null })
  }

  async function handleOpenProject(project: NavProject) {
    setProjectMenu(null)
    await onSelectProject(project.id)
    onNavigate?.('/tasks')
  }

  async function handleCreateTask(project: NavProject) {
    setProjectMenu(null)
    if (onCreateTaskForProject) {
      await onCreateTaskForProject(project.id)
      return
    }
    await handleOpenProject(project)
  }

  function handleProjectSettings() {
    setProjectMenu(null)
    onNavigate?.('/settings/projects')
  }

  function openProjectMembers(project: NavProject) {
    setProjectMenu(null)
    setMembersProject(project)
  }

  async function handleCopyProjectValue(value: string, successMessage: string) {
    setProjectMenu(null)
    try {
      await copyToClipboard(value)
      setCopyMessage(successMessage)
    } catch {
      setCopyMessage('Could not copy. Open project settings and copy it from there.')
    }
  }

  async function handleSaveTeam(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!teamEditor) return

    const name = teamEditor.name.trim()
    if (!name) {
      setTeamEditor({ ...teamEditor, error: 'Enter a team name, then save again.' })
      return
    }

    setTeamEditor({ ...teamEditor, name, saving: true, error: null })
    try {
      await onUpdateTeam(teamEditor.team.id, { name })
      setTeamEditor(null)
    } catch (err) {
      setTeamEditor({
        ...teamEditor,
        name,
        saving: false,
        error: renameErrorMessage('team', err),
      })
    }
  }

  async function handleSaveProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!projectEditor) return

    const name = projectEditor.name.trim()
    if (!name) {
      setProjectEditor({ ...projectEditor, error: 'Enter a project name, then save again.' })
      return
    }

    setProjectEditor({ ...projectEditor, name, saving: true, error: null })
    try {
      await onUpdateProject(projectEditor.project.id, { name })
      setProjectEditor(null)
    } catch (err) {
      setProjectEditor({
        ...projectEditor,
        name,
        saving: false,
        error: renameErrorMessage('project', err),
      })
    }
  }

  async function handleDeleteTeam(team: NavTeam) {
    setTeamMenu(null)
    const confirmed = window.confirm(
      `Delete team "${team.name}"? Projects in this team will also be removed from the sidebar.`
    )
    if (!confirmed) return
    await onDeleteTeam(team.id)
  }

  async function handleDeleteProject(project: NavProject) {
    setProjectMenu(null)
    const confirmed = window.confirm(
      `Delete project "${project.name}"? The project disappears from this workspace, but agents are moved out instead of deleted.`
    )
    if (!confirmed) return
    await onDeleteProject(project.id)
  }

  return (
    <div className="flex flex-col gap-0.5 px-1">
      {teams.map((team) => {
        const expanded = expandedTeams.includes(team.id)
        const teamProjects = projects[team.id] ?? []

        return (
          <div key={team.id}>
            <button
              data-testid={`team-${team.id}`}
              onClick={() => onToggleTeam(team.id)}
              onContextMenu={(event) => handleTeamContextMenu(event, team)}
              className={cn(
                'w-full flex items-center gap-1.5 px-2 py-1 rounded-md text-ui-caption',
                'text-secondary-light dark:text-secondary-dark',
                'hover:bg-black/[0.04] dark:hover:bg-white/[0.06] transition-colors'
              )}
            >
              <span className="w-3 text-ui-caption">{expanded ? '▾' : '▸'}</span>
              <span className="font-medium truncate">{team.name}</span>
            </button>

            {expanded && (
              <div className="ml-4 flex flex-col gap-0.5">
                {teamProjects.length === 0 ? (
                  <EmptyTreeHint
                    testId={`team-${team.id}-empty-projects`}
                    Icon={FolderPlus}
                    title="Add this team's first project"
                    detail="Projects hold tasks, agents, and work lanes for the team."
                    actionLabel="Open Project Settings"
                    onAction={onNavigate ? () => onNavigate('/settings/projects') : undefined}
                  />
                ) : (
                  teamProjects.map((project) => (
                    <button
                      key={project.id}
                      data-testid={`project-${project.id}`}
                      onClick={() => onSelectProject(project.id)}
                      onContextMenu={(event) => handleProjectContextMenu(event, team, project)}
                      className={cn(
                        'w-full flex items-center gap-2 px-2 py-1 rounded-md text-ui-caption text-left transition-colors',
                        selectedProjectId === project.id
                          ? 'bg-apple-blue/10 text-apple-blue font-medium'
                          : 'text-foreground-light dark:text-foreground-dark hover:bg-black/[0.04] dark:hover:bg-white/[0.06]'
                      )}
                    >
                      <span
                        className="w-2 h-2 rounded-full flex-shrink-0"
                        style={{ backgroundColor: project.color || '#007AFF' }}
                      />
                      <span className="truncate">{project.name}</span>
                    </button>
                  ))
                )}
              </div>
            )}
          </div>
        )
      })}

      {teamMenu && (
        <div
          data-testid="team-context-layer"
          className="fixed inset-0 z-40"
          onClick={() => setTeamMenu(null)}
          onContextMenu={(event) => event.preventDefault()}
        >
          <div
            role="menu"
            aria-label={`${teamMenu.team.name} team menu`}
            data-testid="team-context-menu"
            className={cn(
              'fixed min-w-[11rem] rounded-lg border p-1 shadow-lg',
              'bg-white dark:bg-[#2c2c2e]',
              'border-black/10 dark:border-white/10'
            )}
            style={getMenuPosition(teamMenu)}
            onClick={(event) => event.stopPropagation()}
          >
            {canManageTeam(teamMenu.team) && (
              <button
                type="button"
                role="menuitem"
                className="w-full rounded-md px-2.5 py-1.5 text-left text-ui-caption text-foreground-light hover:bg-black/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.06]"
                onClick={() => openTeamEditor(teamMenu.team)}
              >
                Edit team details
              </button>
            )}
            {canDeleteTeam(teamMenu.team) && (
              <button
                type="button"
                role="menuitem"
                className="w-full rounded-md px-2.5 py-1.5 text-left text-ui-caption text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-900/20"
                onClick={() => void handleDeleteTeam(teamMenu.team)}
              >
                Delete Team
              </button>
            )}
          </div>
        </div>
      )}

      {projectMenu && (
        <div
          data-testid="project-context-layer"
          className="fixed inset-0 z-40"
          onClick={() => setProjectMenu(null)}
          onContextMenu={(event) => event.preventDefault()}
        >
          <div
            role="menu"
            aria-label={`${projectMenu.project.name} project menu`}
            data-testid="project-context-menu"
            className={cn(
              'fixed max-h-[calc(100vh-16px)] w-[17.5rem] max-w-[calc(100vw-16px)] overflow-y-auto rounded-xl border p-1 shadow-lg',
              'bg-white dark:bg-[#2c2c2e]',
              'border-black/10 dark:border-white/10'
            )}
            style={getMenuPosition(projectMenu, PROJECT_MENU_SIZE)}
            onClick={(event) => event.stopPropagation()}
          >
            <div className="mb-1 rounded-lg px-2.5 py-2">
              <div className="flex min-w-0 items-center gap-2">
                <span
                  className="h-2.5 w-2.5 shrink-0 rounded-full ring-2 ring-black/5 dark:ring-white/10"
                  style={{ backgroundColor: projectMenu.project.color || '#007AFF' }}
                  aria-hidden="true"
                />
                <span className="truncate text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark">
                  {projectMenu.project.name}
                </span>
              </div>
              <p className="mt-0.5 truncate text-ui-caption text-secondary-light dark:text-secondary-dark">
                {projectMenu.team.name} team · link name {projectMenu.project.slug}
              </p>
            </div>

            <ProjectMenuItem
              Icon={FolderOpen}
              label="Open project board"
              detail="Show this project's tasks"
              tone="primary"
              onClick={() => void handleOpenProject(projectMenu.project)}
            />
            <ProjectMenuItem
              Icon={ListPlus}
              label="Create task here"
              detail="Start work in this project"
              onClick={() => void handleCreateTask(projectMenu.project)}
            />
            {canManageProject(projectMenu.project) && (
              <>
                <ProjectMenuItem
                  Icon={Users}
                  label="Share project"
                  detail="Invite people and choose roles"
                  onClick={() => openProjectMembers(projectMenu.project)}
                />
                <ProjectMenuItem
                  Icon={Pencil}
                  label="Rename project"
                  detail="Change the name people see"
                  onClick={() => openProjectEditor(projectMenu.project)}
                />
              </>
            )}
            {onNavigate && (
              <ProjectMenuItem
                Icon={Settings}
                label="All project settings"
                detail="Open the full settings page"
                onClick={handleProjectSettings}
              />
            )}
            <div className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]" />
            <ProjectMenuItem
              Icon={Copy}
              label="Copy support reference"
              detail="Only share this if Support asks for it"
              onClick={() =>
                void handleCopyProjectValue(
                  projectMenu.project.id,
                  'Project support reference copied'
                )
              }
            />
            <ProjectMenuItem
              Icon={Hash}
              label="Copy link name"
              detail={`${projectMenu.project.slug} · appears in project links`}
              onClick={() =>
                void handleCopyProjectValue(projectMenu.project.slug, 'Project link name copied')
              }
            />
            {canDeleteProject(projectMenu.project) && (
              <>
                <div className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]" />
                <ProjectMenuItem
                  Icon={Trash2}
                  label="Delete Project"
                  detail="Remove project, not the whole team"
                  tone="danger"
                  onClick={() => void handleDeleteProject(projectMenu.project)}
                />
              </>
            )}
          </div>
        </div>
      )}

      {teamEditor && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="Close team details"
            className="absolute inset-0 bg-black/40"
            onClick={() => setTeamEditor(null)}
          />
          <form
            role="dialog"
            aria-modal="true"
            aria-labelledby="team-details-title"
            className="relative w-[360px] rounded-lg bg-white p-5 shadow-xl dark:bg-[#2c2c2e]"
            onSubmit={handleSaveTeam}
          >
            <h2
              id="team-details-title"
              className="mb-4 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark"
            >
              Edit team details
            </h2>
            {teamEditor.error && (
              <div className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-ui-caption text-red-600 dark:bg-red-900/20 dark:text-red-400">
                {teamEditor.error}
              </div>
            )}
            <label
              htmlFor="team-config-name"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Team name people see
            </label>
            <input
              id="team-config-name"
              value={teamEditor.name}
              onChange={(event) =>
                setTeamEditor({ ...teamEditor, name: event.target.value, error: null })
              }
              className="w-full rounded-lg bg-apple-gray-6 px-3 py-2 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue/30 dark:bg-white/[0.06] dark:text-foreground-dark"
              autoFocus
            />
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                disabled={teamEditor.saving}
                onClick={() => setTeamEditor(null)}
                className="rounded-full bg-apple-gray-5 px-3 py-1.5 text-ui-button font-medium text-foreground-light dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={teamEditor.saving}
                className="rounded-full bg-apple-blue px-3 py-1.5 text-ui-button font-medium text-white disabled:opacity-50"
              >
                {teamEditor.saving ? 'Saving…' : 'Save'}
              </button>
            </div>
          </form>
        </div>
      )}

      {projectEditor && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="Close project configuration"
            className="absolute inset-0 bg-black/40"
            onClick={() => setProjectEditor(null)}
          />
          <form
            role="dialog"
            aria-modal="true"
            aria-labelledby="project-config-title"
            className="relative w-[360px] rounded-lg bg-white p-5 shadow-xl dark:bg-[#2c2c2e]"
            onSubmit={handleSaveProject}
          >
            <h2
              id="project-config-title"
              className="mb-4 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark"
            >
              Rename project
            </h2>
            {projectEditor.error && (
              <div className="mb-3 rounded-lg bg-red-50 px-3 py-2 text-ui-caption text-red-600 dark:bg-red-900/20 dark:text-red-400">
                {projectEditor.error}
              </div>
            )}
            <label
              htmlFor="project-config-name"
              className="mb-1 block text-ui-caption font-medium text-secondary-light dark:text-secondary-dark"
            >
              Project name people see
            </label>
            <input
              id="project-config-name"
              value={projectEditor.name}
              onChange={(event) =>
                setProjectEditor({ ...projectEditor, name: event.target.value, error: null })
              }
              className="w-full rounded-lg bg-apple-gray-6 px-3 py-2 text-ui-body text-foreground-light outline-none focus:ring-2 focus:ring-apple-blue/30 dark:bg-white/[0.06] dark:text-foreground-dark"
              autoFocus
            />
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                disabled={projectEditor.saving}
                onClick={() => setProjectEditor(null)}
                className="rounded-full bg-apple-gray-5 px-3 py-1.5 text-ui-button font-medium text-foreground-light dark:bg-white/[0.06] dark:text-foreground-dark"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={projectEditor.saving}
                className="rounded-full bg-apple-blue px-3 py-1.5 text-ui-button font-medium text-white disabled:opacity-50"
              >
                {projectEditor.saving ? 'Saving…' : 'Save'}
              </button>
            </div>
          </form>
        </div>
      )}

      {membersProject && (
        <ResourceMembersModal
          resourceLabel="Project"
          resourceName={membersProject.name}
          loadMembers={loadSelectedProjectMembers}
          loadUsers={loadOrgUsers}
          addMember={addSelectedProjectMember}
          updateMember={updateSelectedProjectMember}
          removeMember={removeSelectedProjectMember}
          onClose={() => setMembersProject(null)}
        />
      )}

      {copyMessage && (
        <div
          role="status"
          aria-live="polite"
          data-testid="project-copy-status"
          className={cn(
            'fixed bottom-4 left-1/2 z-50 -translate-x-1/2 rounded-full px-4 py-2 text-ui-caption font-medium shadow-lg',
            'bg-foreground-light text-white dark:bg-foreground-dark dark:text-black'
          )}
        >
          {copyMessage}
        </div>
      )}
    </div>
  )
}

async function copyToClipboard(value: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.setAttribute('readonly', 'true')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  textarea.remove()
}
