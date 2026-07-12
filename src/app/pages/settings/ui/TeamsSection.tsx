import { type ReactNode, useCallback, useEffect, useState } from 'react'
import { CheckCircle2, FolderKanban, Plus, ShieldAlert, Users } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
import {
  ResourceMembersModal,
  resourceMemberSelectionLostMessage,
} from '@app/features/manage-members'
import { CreateTeamForm, EditableTeamRow } from '@app/features/manage-team'
import { userApi } from '@app/entities/user'
import { teamApi, type NavTeam, type UpdateTeamInput } from '@app/entities/navigation/team'
import { workspaceSettingsErrorMessage } from '../model/workspaceSettingsErrorMessage'

export function TeamsSection() {
  const { user } = useAuth()
  const hasTeamSpace = Boolean(user?.orgId)
  const canCreateTeam = hasTeamSpace && (user?.role === 'owner' || user?.role === 'admin')
  const [teams, setTeams] = useState<NavTeam[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)
  const [membersTeam, setMembersTeam] = useState<NavTeam | null>(null)
  const [createdTeam, setCreatedTeam] = useState<NavTeam | null>(null)
  const loadOrgUsers = useCallback(() => userApi.getUsers(), [])

  const loadTeams = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId) return
    setLoading(true)
    setError(null)
    try {
      const result = await teamApi.getTeams(orgId)
      setTeams(result)
      setCreatedTeam((current) =>
        current && result.some((team) => team.id === current.id) ? current : null
      )
    } catch (err) {
      setError(workspaceSettingsErrorMessage('team', 'load', err))
    } finally {
      setLoading(false)
    }
  }, [user?.orgId])

  useEffect(() => {
    void loadTeams()
  }, [loadTeams])

  async function handleCreate(name: string) {
    const orgId = user?.orgId
    if (!orgId) return
    setSaving(true)
    setError(null)
    try {
      const team = await teamApi.createTeam(orgId, { name })
      setTeams((prev) => [...prev, team])
      setCreatedTeam(team)
      setShowForm(false)
    } catch (err) {
      const message = workspaceSettingsErrorMessage('team', 'create', err)
      throw new Error(message, { cause: err })
    } finally {
      setSaving(false)
    }
  }

  async function handleUpdate(teamId: string, input: UpdateTeamInput) {
    const orgId = user?.orgId
    if (!orgId) return
    const updated = await teamApi.updateTeam(orgId, teamId, input)
    setTeams((prev) => prev.map((team) => (team.id === teamId ? { ...team, ...updated } : team)))
    setCreatedTeam((current) => (current?.id === teamId ? { ...current, ...updated } : current))
  }

  async function handleDelete(teamId: string) {
    const orgId = user?.orgId
    if (!orgId) return
    await teamApi.deleteTeam(orgId, teamId)
    setTeams((prev) => prev.filter((team) => team.id !== teamId))
    setMembersTeam((current) => (current?.id === teamId ? null : current))
    setCreatedTeam((current) => (current?.id === teamId ? null : current))
  }

  function startTeamCreate() {
    setCreatedTeam(null)
    setShowForm(true)
  }

  const teamEmptyTitle = !hasTeamSpace
    ? 'Choose a team space first'
    : canCreateTeam
      ? 'Create a team first'
      : 'Ask an owner or admin to create the first team'
  const teamEmptyDescription = !hasTeamSpace
    ? 'Teams belong to a team space. Select or create one before adding people.'
    : canCreateTeam
      ? 'Teams keep projects and access together. Start with one team, then add projects inside it.'
      : 'Only owners and admins can create teams. You can work here after someone adds a team for you.'
  const teamEmptySteps = !hasTeamSpace
    ? [
        'Choose a team space from the account menu.',
        'Open Settings, then Teams again.',
        'Choose Teams, then create the team.',
      ]
    : canCreateTeam
      ? [
          'Choose Create first team.',
          'Name it after the people or work area that will share projects.',
          'Create the first project in Projects next.',
        ]
      : [
          'Ask an owner or admin to create one team.',
          'Ask them which team should own the first project.',
          'Come back to Projects after the team appears.',
        ]

  const loadSelectedTeamMembers = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId || !membersTeam) return []
    return teamApi.getMembers(orgId, membersTeam.id)
  }, [membersTeam, user?.orgId])

  const addSelectedTeamMember = useCallback(
    async (input: Parameters<typeof teamApi.addMember>[2]) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error(resourceMemberSelectionLostMessage('Team'))
      return teamApi.addMember(orgId, membersTeam.id, input)
    },
    [membersTeam, user?.orgId]
  )

  const updateSelectedTeamMember = useCallback(
    async (userId: string, input: Parameters<typeof teamApi.updateMember>[3]) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error(resourceMemberSelectionLostMessage('Team'))
      return teamApi.updateMember(orgId, membersTeam.id, userId, input)
    },
    [membersTeam, user?.orgId]
  )

  const removeSelectedTeamMember = useCallback(
    async (userId: string) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error(resourceMemberSelectionLostMessage('Team'))
      return teamApi.removeMember(orgId, membersTeam.id, userId)
    },
    [membersTeam, user?.orgId]
  )

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Teams and access</h2>
          <p className={uiStyles.sectionDescription}>
            {teams.length} {teams.length === 1 ? 'team keeps' : 'teams keep'} people and projects
            together inside this team space. Open Manage people on a team to add people or change
            access.
          </p>
          {!canCreateTeam && teams.length > 0 && (
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Only owners and admins can create another team. Ask one of them if you need a new team
              here.
            </p>
          )}
        </div>
        {!showForm && canCreateTeam && (
          <button type="button" onClick={startTeamCreate} className={uiStyles.primaryButton}>
            <Plus size={14} strokeWidth={2} aria-hidden="true" />
            <span>New team</span>
          </button>
        )}
      </div>

      {error && (
        <div role="alert" aria-live="polite" className={uiStyles.error}>
          {error}
        </div>
      )}

      {createdTeam && !showForm && (
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
                  Team "{createdTeam.name}" is ready
                </p>
                <p className="mt-1 text-ui-caption">
                  Next: create the first project in Projects. Use Manage people only when this team
                  needs direct access before project work starts.
                </p>
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <a href="/settings/projects" className={uiStyles.primaryButton}>
                <FolderKanban size={14} strokeWidth={2} aria-hidden="true" />
                <span>Create first project</span>
              </a>
              <button
                type="button"
                onClick={() => setMembersTeam(createdTeam)}
                className={uiStyles.secondaryButton}
              >
                <Users size={14} strokeWidth={2} aria-hidden="true" />
                <span>Manage people</span>
              </button>
            </div>
          </div>
        </div>
      )}

      <div className={cn(uiStyles.card)}>
        {loading && teams.length === 0 ? (
          <BeginnerLoadingState
            framed={false}
            title="Checking teams"
            detail="Forge is checking which teams are available in this team space."
            nextStep="If this takes more than a moment, open Teams again or ask an owner or admin to check team access."
            success="Success looks like a team row or a Create first team step."
          />
        ) : !hasTeamSpace ? (
          <WorkspaceEmptyState
            icon={<ShieldAlert size={18} strokeWidth={2} aria-hidden="true" />}
            title={teamEmptyTitle}
            description={teamEmptyDescription}
            steps={teamEmptySteps}
          />
        ) : teams.length === 0 && !showForm ? (
          <WorkspaceEmptyState
            icon={<Users size={18} strokeWidth={2} aria-hidden="true" />}
            title={teamEmptyTitle}
            description={teamEmptyDescription}
            steps={teamEmptySteps}
            action={
              canCreateTeam ? (
                <button type="button" onClick={startTeamCreate} className={uiStyles.primaryButton}>
                  <Plus size={14} strokeWidth={2} aria-hidden="true" />
                  <span>Create first team</span>
                </button>
              ) : null
            }
          />
        ) : (
          teams.map((team) => (
            <EditableTeamRow
              key={team.id}
              team={team}
              onUpdate={handleUpdate}
              onDelete={handleDelete}
              onManageMembers={setMembersTeam}
            />
          ))
        )}

        {showForm && canCreateTeam && (
          <CreateTeamForm
            onSave={handleCreate}
            onCancel={() => setShowForm(false)}
            saving={saving}
          />
        )}
      </div>

      {membersTeam && user?.orgId && (
        <ResourceMembersModal
          resourceLabel="Team"
          resourceName={membersTeam.name}
          loadMembers={loadSelectedTeamMembers}
          loadUsers={loadOrgUsers}
          addMember={addSelectedTeamMember}
          updateMember={updateSelectedTeamMember}
          removeMember={removeSelectedTeamMember}
          onClose={() => setMembersTeam(null)}
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
  steps: string[]
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
        <ol className="mt-3 list-decimal space-y-1 pl-4 text-left text-ui-caption text-secondary-light dark:text-secondary-dark">
          {steps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </div>
      {action}
    </div>
  )
}
