import { useCallback, useEffect, useState } from 'react'
import { Plus } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { ResourceMembersModal } from '@app/features/manage-members'
import { CreateProjectForm, EditableProjectRow } from '@app/features/manage-project'
import { projectApi, type NavProject, type UpdateProjectInput } from '@app/entities/project'
import { teamApi, type NavTeam } from '@app/entities/team'
import { userApi } from '@app/entities/user'

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
      setError(err instanceof Error ? err.message : 'Failed to load projects')
    } finally {
      setLoading(false)
    }
  }, [user?.orgId])

  useEffect(() => {
    void loadData()
  }, [loadData])

  async function handleCreate(name: string, teamId: string) {
    setSaving(true)
    setError(null)
    try {
      const project = await projectApi.createProject(teamId, { name })
      const team = teams.find((t) => t.id === teamId)
      setProjectsWithTeam((prev) => [...prev, { project, teamName: team?.name ?? '' }])
      setShowForm(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create project')
    } finally {
      setSaving(false)
    }
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
        {!showForm && projectCreatableTeams.length > 0 && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <Plus size={14} strokeWidth={2} aria-hidden="true" />
            <span>New Project</span>
          </button>
        )}
      </div>

      {error && <div className={uiStyles.error}>{error}</div>}

      <div className={cn(uiStyles.card)}>
        {loading && projectsWithTeam.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading projects…
          </div>
        ) : !user?.orgId ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No organization found
            </p>
          </div>
        ) : projectsWithTeam.length === 0 && !showForm ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No projects yet
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Create a project to start organizing your agents
            </p>
          </div>
        ) : (
          projectsWithTeam.map(({ project, teamName }) => (
            <EditableProjectRow
              key={project.id}
              project={project}
              teamName={teamName}
              onUpdate={handleUpdate}
              onDelete={handleDelete}
              onManageMembers={setMembersProject}
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
