import { type ReactNode, useCallback, useEffect, useState } from 'react'
import { ShieldAlert, Users } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAuth } from '@app/shared/model/auth.context'
import { ResourceMembersModal } from '@app/features/manage-members'
import { CreateTeamForm, EditableTeamRow } from '@app/features/manage-team'
import { userApi } from '@app/entities/user'
import { teamApi, type NavTeam, type UpdateTeamInput } from '@app/entities/team'

export function TeamsSection() {
  const { user } = useAuth()
  const canCreateTeam = user?.role === 'owner' || user?.role === 'admin'
  const [teams, setTeams] = useState<NavTeam[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [showForm, setShowForm] = useState(false)
  const [saving, setSaving] = useState(false)
  const [membersTeam, setMembersTeam] = useState<NavTeam | null>(null)
  const emptyTeamTitle = !user?.orgId
    ? 'Choose an organization first'
    : canCreateTeam
      ? 'Create a team first'
      : 'Ask an owner or admin to create the first team'
  const emptyTeamDescription = !user?.orgId
    ? 'Teams belong to an organization. Switch to one before adding projects or agents.'
    : canCreateTeam
      ? 'Teams group projects and decide who can manage work. Start with one team, then add projects inside it.'
      : 'Only owners and admins can create teams. You can work here after someone adds a team for you.'

  const loadOrgUsers = useCallback(() => userApi.getUsers(), [])

  const loadTeams = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId) return
    setLoading(true)
    setError(null)
    try {
      const result = await teamApi.getTeams(orgId)
      setTeams(result)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load teams')
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
      setShowForm(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create team')
    } finally {
      setSaving(false)
    }
  }

  async function handleUpdate(teamId: string, input: UpdateTeamInput) {
    const orgId = user?.orgId
    if (!orgId) return
    const updated = await teamApi.updateTeam(orgId, teamId, input)
    setTeams((prev) => prev.map((team) => (team.id === teamId ? { ...team, ...updated } : team)))
  }

  async function handleDelete(teamId: string) {
    const orgId = user?.orgId
    if (!orgId) return
    await teamApi.deleteTeam(orgId, teamId)
    setTeams((prev) => prev.filter((team) => team.id !== teamId))
    setMembersTeam((current) => (current?.id === teamId ? null : current))
  }

  const loadSelectedTeamMembers = useCallback(async () => {
    const orgId = user?.orgId
    if (!orgId || !membersTeam) return []
    return teamApi.getMembers(orgId, membersTeam.id)
  }, [membersTeam, user?.orgId])

  const addSelectedTeamMember = useCallback(
    async (input: Parameters<typeof teamApi.addMember>[2]) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error('No team selected')
      return teamApi.addMember(orgId, membersTeam.id, input)
    },
    [membersTeam, user?.orgId]
  )

  const updateSelectedTeamMember = useCallback(
    async (userId: string, input: Parameters<typeof teamApi.updateMember>[3]) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error('No team selected')
      return teamApi.updateMember(orgId, membersTeam.id, userId, input)
    },
    [membersTeam, user?.orgId]
  )

  const removeSelectedTeamMember = useCallback(
    async (userId: string) => {
      const orgId = user?.orgId
      if (!orgId || !membersTeam) throw new Error('No team selected')
      return teamApi.removeMember(orgId, membersTeam.id, userId)
    },
    [membersTeam, user?.orgId]
  )

  return (
    <div>
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Teams</h2>
          <p className={uiStyles.sectionDescription}>
            {teams.length} {teams.length === 1 ? 'team' : 'teams'} in this organization
          </p>
        </div>
        {!showForm && canCreateTeam && teams.length > 0 && (
          <button
            type="button"
            onClick={() => setShowForm(true)}
            className={uiStyles.primaryButton}
          >
            <Users size={14} strokeWidth={2} aria-hidden="true" />
            <span>New Team</span>
          </button>
        )}
      </div>

      {error && <div className={uiStyles.error}>{error}</div>}

      <div className={cn(uiStyles.card)}>
        {loading && teams.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading teams…
          </div>
        ) : !user?.orgId ? (
          <WorkspaceEmptyState
            icon={<ShieldAlert size={18} strokeWidth={2} aria-hidden="true" />}
            title={emptyTeamTitle}
            description={emptyTeamDescription}
          />
        ) : teams.length === 0 && !showForm ? (
          <WorkspaceEmptyState
            icon={<Users size={18} strokeWidth={2} aria-hidden="true" />}
            title={emptyTeamTitle}
            description={emptyTeamDescription}
            action={
              canCreateTeam ? (
                <button
                  type="button"
                  onClick={() => setShowForm(true)}
                  className={uiStyles.primaryButton}
                >
                  <Users size={14} strokeWidth={2} aria-hidden="true" />
                  <span>New Team</span>
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
