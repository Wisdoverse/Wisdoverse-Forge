import { type ReactNode, useCallback, useEffect, useState } from 'react'
import { FolderKanban, ShieldAlert, Users } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { ResourceMembersModal } from '@app/features/manage-members'
import { CreateProjectForm, EditableProjectRow } from '@app/features/manage-project'
import {
  projectApi,
  type CloneSummary,
  type NavProject,
  type UpdateProjectInput,
} from '@app/entities/project'
import { teamApi, type NavTeam } from '@app/entities/team'
import { userApi } from '@app/entities/user'
import { workspaceSettingsErrorMessage } from '../model/workspaceSettingsErrorMessage'

interface ProjectWithTeam {
  project: NavProject
  teamName: string
}

export function ProjectsSection() {
  const { user } = useAuth()
  const [teams, setTeams] = useState<NavTeam[]>([])
  const [projectsWithTeam, setProjectsWithTeam] = useState<ProjectWithTeam[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)
  const [membersProject, setMembersProject] = useState<NavProject | null>(null)
  const projectCreatableTeams = teams.filter((team) => team.canCreateProject !== false)
  const hasTeams = teams.length > 0
  const canCreateProject = projectCreatableTeams.length > 0
  const projectEmptyTitle = !user?.orgId
    ? 'Choose a team space first'
    : !hasTeams
      ? 'Create a team before adding projects'
      : canCreateProject
        ? 'Create your first project'
        : 'Ask a team admin to let you create projects'
  const projectEmptyDescription = !user?.orgId
    ? 'Projects belong to teams inside a team space. Switch to one before setting up work.'
    : !hasTeams
      ? 'Projects live inside teams. Open Teams first, create one team, then come back here.'
      : canCreateProject
        ? 'Projects keep tasks, agents, and members together for one area of work.'
        : 'You can see teams, but none of them allow you to create projects yet.'

  const loadOrgUsers = useCallback(() => userApi.getUsers(), [])

  const loadData = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId) return
    setLoading(true)
    setError(null)
    try {
      const loadedTeams = await teamApi.getTeams(orgId)
      setTeams(loadedTeams)

      const projectResults = await Promise.all(
        loadedTeams.map(async (team) => {
          try {
            const projects = await projectApi.getProjects(team.id)
            return projects.map((p) => ({ project: p, teamName: team.name }))
          } catch {
            return []
          }
        })
      )
      setProjectsWithTeam(projectResults.flat())
    } catch (err) {
      setError(workspaceSettingsErrorMessage('project', 'load', err))
    } finally {
      setLoading(false)
    }
  }, [user?.orgId])

  useEffect(() => {
    void loadData()
  }, [loadData])

  async function handleCreate(name: string, teamId: string, repositoryUrl?: string) {
    setSaving(true)
    setError(null)
    try {
      const project = await projectApi.createProject(teamId, { name, repositoryUrl })
      const team = teams.find((t) => t.id === teamId)
      setProjectsWithTeam((prev) => [...prev, { project, teamName: team?.name ?? '' }])
      setShowForm(false)
    } catch (err) {
      // Re-throw so CreateProjectForm surfaces the server's rejection (e.g. an
      // invalid repository URL) as a banner instead of failing silently.
      const message = workspaceSettingsErrorMessage('project', 'create', err)
      setError(message)
      throw new Error(message, { cause: err })
    } finally {
      setSaving(false)
    }
  }

  function handleCloneRetried(projectId: string, summary: CloneSummary) {
    setProjectsWithTeam((prev) =>
      prev.map((item) =>
        item.project.id === projectId
          ? { ...item, project: { ...item.project, cloneStatus: 'queued', clone: summary } }
          : item
      )
    )
  }

  async function handleUpdate(project: NavProject, input: UpdateProjectInput) {
    const updated = await projectApi.updateProject(project.teamId, project.id, input)
    setProjectsWithTeam((prev) =>
      prev.map((item) =>
        item.project.id === project.id
          ? { ...item, project: { ...item.project, ...updated } }
          : item
      )
    )
  }

  async function handleDelete(project: NavProject) {
    await projectApi.deleteProject(project.teamId, project.id)
    setProjectsWithTeam((prev) => prev.filter((item) => item.project.id !== project.id))
    setMembersProject((current) => (current?.id === project.id ? null : current))
  }

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

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Projects</h2>
          <p className={uiStyles.sectionDescription}>
            {projectsWithTeam.length} {projectsWithTeam.length === 1 ? 'project' : 'projects'}{' '}
            across {teams.length} {teams.length === 1 ? 'team' : 'teams'}
          </p>
        </div>
        {!showForm && canCreateProject && projectsWithTeam.length > 0 && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <FolderKanban size={14} strokeWidth={2} aria-hidden="true" />
            <span>New Project</span>
          </button>
        )}
      </div>

      {error && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {error}
        </div>
      )}

      <div className={cn(uiStyles.card)}>
        {loading && projectsWithTeam.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading projects…
          </div>
        ) : !user?.orgId ? (
          <WorkspaceEmptyState
            icon={<ShieldAlert size={18} strokeWidth={2} aria-hidden="true" />}
            title={projectEmptyTitle}
            description={projectEmptyDescription}
          />
        ) : projectsWithTeam.length === 0 && !showForm ? (
          <WorkspaceEmptyState
            icon={
              hasTeams ? (
                <FolderKanban size={18} strokeWidth={2} aria-hidden="true" />
              ) : (
                <Users size={18} strokeWidth={2} aria-hidden="true" />
              )
            }
            title={projectEmptyTitle}
            description={projectEmptyDescription}
            action={
              canCreateProject ? (
                <button
                  type="button"
                  onClick={() => setShowForm(true)}
                  className={uiStyles.primaryButton}
                >
                  <FolderKanban size={14} strokeWidth={2} aria-hidden="true" />
                  <span>New Project</span>
                </button>
              ) : null
            }
          />
        ) : (
          projectsWithTeam.map(({ project, teamName }) => (
            <EditableProjectRow
              key={project.id}
              project={project}
              teamName={teamName}
              onUpdate={handleUpdate}
              onDelete={handleDelete}
              onManageMembers={setMembersProject}
              onCloneRetried={handleCloneRetried}
            />
          ))
        )}

        {showForm && projectCreatableTeams.length > 0 && (
          <CreateProjectForm
            teams={projectCreatableTeams}
            onSave={handleCreate}
            onCancel={() => setShowForm(false)}
            saving={saving}
          />
        )}
      </div>

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

function WorkspaceEmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode
  title: string
  description: string
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-center gap-3 px-4 py-8 text-center">
      <div
        className="flex h-10 w-10 items-center justify-center rounded-lg bg-black/[0.03] text-secondary-light ring-1 ring-black/5 dark:bg-white/[0.05] dark:text-secondary-dark dark:ring-white/10"
        aria-hidden="true"
      >
        {icon}
      </div>
      <div className="max-w-md">
        <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {title}
        </p>
        <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
          {description}
        </p>
      </div>
      {action}
    </div>
  )
}
