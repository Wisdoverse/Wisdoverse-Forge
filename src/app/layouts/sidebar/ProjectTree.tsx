import { useCallback, useEffect, useState, type FormEvent, type MouseEvent } from 'react'
import {
  Copy,
  FolderOpen,
  Hash,
  ListPlus,
  Pencil,
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

const TEAM_MENU_SIZE = { width: 190, height: 108 }
const PROJECT_MENU_SIZE = { width: 280, height: 456 }

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

  if (teams.length === 0) {
    return (
      <p className="px-4 py-3 text-ui-caption text-secondary-light dark:text-secondary-dark">
        No teams yet
      </p>
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

  async function handleCopyProjectValue(value: string) {
    setProjectMenu(null)
    try {
      await copyToClipboard(value)
    } catch {
      // Copy is a convenience action; do not block the menu on browser support.
    }
  }

  async function handleSaveTeam(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!teamEditor) return

    const name = teamEditor.name.trim()
    if (!name) {
      setTeamEditor({ ...teamEditor, error: 'Team name is required' })
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
        error: err instanceof Error ? err.message : 'Failed to update team',
      })
    }
  }

  async function handleSaveProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!projectEditor) return

    const name = projectEditor.name.trim()
    if (!name) {
      setProjectEditor({ ...projectEditor, error: 'Project name is required' })
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
        error: err instanceof Error ? err.message : 'Failed to update project',
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
      `Delete project "${project.name}"? Agents will be moved out of this project.`
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
                  <p className="px-2 py-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
                    No projects
                  </p>
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
                Configure Team
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
                {projectMenu.team.name} / {projectMenu.project.slug}
              </p>
            </div>

            <ProjectMenuItem
              Icon={FolderOpen}
              label="Open Project"
              detail="Switch board context"
              tone="primary"
              onClick={() => void handleOpenProject(projectMenu.project)}
            />
            <ProjectMenuItem
              Icon={ListPlus}
              label="New Task"
              detail="Create inside this project"
              onClick={() => void handleCreateTask(projectMenu.project)}
            />
            {canManageProject(projectMenu.project) && (
              <>
                <ProjectMenuItem
                  Icon={Users}
                  label="Manage Access"
                  detail="Add people and set roles"
                  onClick={() => openProjectMembers(projectMenu.project)}
                />
                <ProjectMenuItem
                  Icon={Pencil}
                  label="Configure Project"
                  detail="Rename and tune basics"
                  onClick={() => openProjectEditor(projectMenu.project)}
                />
              </>
            )}
            {onNavigate && (
              <ProjectMenuItem
                Icon={Settings}
                label="Project Settings"
                detail="Open Settings / Projects"
                onClick={handleProjectSettings}
              />
            )}
            <div className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]" />
            <ProjectMenuItem
              Icon={Copy}
              label="Copy Project ID"
              detail={projectMenu.project.id}
              onClick={() => void handleCopyProjectValue(projectMenu.project.id)}
            />
            <ProjectMenuItem
              Icon={Hash}
              label="Copy Slug"
              detail={projectMenu.project.slug}
              onClick={() => void handleCopyProjectValue(projectMenu.project.slug)}
            />
            {canDeleteProject(projectMenu.project) && (
              <>
                <div className="my-1 h-px bg-black/[0.06] dark:bg-white/[0.08]" />
                <ProjectMenuItem
                  Icon={Trash2}
                  label="Delete Project"
                  detail="Remove from this workspace"
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
            aria-label="Close team configuration"
            className="absolute inset-0 bg-black/40"
            onClick={() => setTeamEditor(null)}
          />
          <form
            role="dialog"
            aria-modal="true"
            aria-labelledby="team-config-title"
            className="relative w-[360px] rounded-lg bg-white p-5 shadow-xl dark:bg-[#2c2c2e]"
            onSubmit={handleSaveTeam}
          >
            <h2
              id="team-config-title"
              className="mb-4 text-ui-section font-semibold text-foreground-light dark:text-foreground-dark"
            >
              Configure Team
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
              Team Name
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
              Configure Project
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
              Project Name
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
