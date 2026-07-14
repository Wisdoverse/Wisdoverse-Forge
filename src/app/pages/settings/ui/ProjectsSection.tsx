import { type ReactNode, useCallback, useEffect, useState } from 'react'
import { Bot, CheckCircle2, CheckSquare, FolderKanban, ShieldAlert, Users } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import {
  ResourceMembersModal,
  resourceMemberSelectionLostMessage,
} from '@app/features/manage-members'
import { CreateProjectForm, EditableProjectRow } from '@app/features/manage-project'
import {
  projectApi,
  type CloneSummary,
  type NavProject,
  type UpdateProjectInput,
} from '@app/entities/navigation/project'
import { teamApi, type NavTeam } from '@app/entities/navigation/team'
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
  const [createdProjectName, setCreatedProjectName] = useState<string | null>(null)
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
  const projectEmptySteps = !user?.orgId
    ? [
        'Choose a team space from the account menu.',
        'Open Settings, then Projects again.',
        'Choose Projects, then create the project.',
      ]
    : !hasTeams
      ? [
          'Choose Open Teams.',
          'Create one team for the people who share this work.',
          'Come back to Projects and choose New Project.',
        ]
      : canCreateProject
        ? [
            'Choose New Project.',
            'Name it after the app, product, or work area.',
            'Add a code link only when agents need files right away.',
          ]
        : [
            'Ask a team admin which team should own this project.',
            'Ask them to let you create projects in that team.',
            'Come back to Projects after access is updated.',
          ]

  const loadOrgUsers = useCallback(() => userApi.getUsers(), [])

  function startProjectCreate() {
    setCreatedProjectName(null)
    setShowForm(true)
  }

  const loadData = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId) return
    setLoading(true)
    setError(null)
    try {
      const loadedTeams = await teamApi.getTeams(orgId)
      setTeams(loadedTeams)

      let projectLoadError: unknown = null
      const projectResults = await Promise.all(
        loadedTeams.map(async (team) => {
          try {
            const projects = await projectApi.getProjects(team.id)
            return projects.map((p) => ({ project: p, teamName: team.name }))
          } catch (err) {
            projectLoadError ??= err
            return []
          }
        })
      )
      setProjectsWithTeam(projectResults.flat())
      if (projectLoadError) {
        setError(
          `${workspaceSettingsErrorMessage('project', 'load', projectLoadError)} Some projects may be missing below.`
        )
      }
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
      setCreatedProjectName(project.name || name)
      setShowForm(false)
    } catch (err) {
      // Keep create failures in the form the user is editing; the form banner
      // receives this normalized message from the thrown error.
      const message = workspaceSettingsErrorMessage('project', 'create', err)
      setCreatedProjectName(null)
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
      if (!membersProject) throw new Error(resourceMemberSelectionLostMessage('Project'))
      return projectApi.addMember(membersProject.id, input)
    },
    [membersProject]
  )

  const updateSelectedProjectMember = useCallback(
    async (userId: string, input: Parameters<typeof projectApi.updateMember>[2]) => {
      if (!membersProject) throw new Error(resourceMemberSelectionLostMessage('Project'))
      return projectApi.updateMember(membersProject.id, userId, input)
    },
    [membersProject]
  )

  const removeSelectedProjectMember = useCallback(
    async (userId: string) => {
      if (!membersProject) throw new Error(resourceMemberSelectionLostMessage('Project'))
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
            across {teams.length} {teams.length === 1 ? 'team' : 'teams'}. Open Manage people on a
            project to add people or change access.
          </p>
        </div>
        {!showForm && canCreateProject && projectsWithTeam.length > 0 && (
          <button type="button" onClick={startProjectCreate} className={uiStyles.primaryButton}>
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

      {createdProjectName && (
        <div role="status" aria-live="polite" className={cn(uiStyles.note, 'mb-4')}>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex gap-2">
              <CheckCircle2
                size={18}
                strokeWidth={2}
                aria-hidden="true"
                className="mt-0.5 flex-none text-apple-green"
              />
              <div>
                <p className="font-medium text-foreground-light dark:text-foreground-dark">
                  Project "{createdProjectName}" is ready
                </p>
                <p className="mt-1 text-ui-caption">
                  Next: set up a place for new tasks in Agents, then create the first task in Tasks.
                </p>
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <a href="/agents" className={uiStyles.secondaryButton}>
                <Bot size={14} strokeWidth={2} aria-hidden="true" />
                <span>Set up place</span>
              </a>
              <a href="/tasks" className={uiStyles.primaryButton}>
                <CheckSquare size={14} strokeWidth={2} aria-hidden="true" />
                <span>Create first task</span>
              </a>
            </div>
          </div>
        </div>
      )}

      <div className="border-y border-black/[0.06] bg-transparent dark:border-white/[0.08]">
        {loading && projectsWithTeam.length === 0 ? (
          <BeginnerLoadingState
            framed={false}
            title="Checking projects"
            detail="Forge is checking which projects are available for this team space."
            nextStep="If this takes more than a moment, open Projects again or ask an owner or admin to check project access."
            success="Success looks like a project row or a New Project step."
          />
        ) : !user?.orgId ? (
          <WorkspaceEmptyState
            icon={<ShieldAlert size={18} strokeWidth={2} aria-hidden="true" />}
            title={projectEmptyTitle}
            description={projectEmptyDescription}
            steps={projectEmptySteps}
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
            steps={projectEmptySteps}
            action={
              canCreateProject ? (
                <button
                  type="button"
                  onClick={startProjectCreate}
                  className={uiStyles.primaryButton}
                >
                  <FolderKanban size={14} strokeWidth={2} aria-hidden="true" />
                  <span>New Project</span>
                </button>
              ) : !hasTeams ? (
                <a href="/settings/teams" className={uiStyles.primaryButton}>
                  <Users size={14} strokeWidth={2} aria-hidden="true" />
                  <span>Open Teams</span>
                </a>
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
  steps,
  action,
}: {
  icon: ReactNode
  title: string
  description: string
  steps?: string[]
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
        {steps && steps.length > 0 && (
          <ol className="mt-3 list-decimal space-y-1 pl-4 text-left text-ui-caption text-secondary-light dark:text-secondary-dark">
            {steps.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
        )}
      </div>
      {action}
    </div>
  )
}
