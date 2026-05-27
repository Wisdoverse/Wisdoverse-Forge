import { useEffect } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { ResourceProfileOption } from '@app/shared/api/legacy/AgentAPI'

// ============================================================================
// Helpers
// ============================================================================

function formatMemory(memoryMb: number): string {
  if (memoryMb >= 1024) {
    const gb = memoryMb / 1024
    return `${gb % 1 === 0 ? gb : gb.toFixed(1)} GB`
  }
  return `${memoryMb} MB`
}

function formatCpu(cpu: number): string {
  return `${cpu} ${cpu === 1 ? 'core' : 'cores'}`
}

function describeProfile(profile: ResourceProfileOption): string {
  if (profile.cpu <= 1 && profile.memoryMb <= 2048) {
    return 'Best for short chats, planning, and small file edits.'
  }

  if (profile.cpu <= 2 && profile.memoryMb <= 4096) {
    return 'Good default for everyday coding and review tasks.'
  }

  return 'Use for larger builds, long searches, or heavy project work.'
}

// ============================================================================
// Profile Row
// ============================================================================

interface ProfileRowProps {
  profile: ResourceProfileOption
}

function ProfileRow({ profile }: ProfileRowProps) {
  return (
    <tr className={uiStyles.row}>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
          {profile.name}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {describeProfile(profile)}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatCpu(profile.cpu)} power · {formatMemory(profile.memoryMb)} memory
        </span>
      </td>
    </tr>
  )
}

// ============================================================================
// ResourcesSection
// ============================================================================

export function ResourcesSection() {
  const { resourceProfiles, resourceProfilesLoading, resourceProfilesError, loadResourceProfiles } =
    useSettingsStore()

  useEffect(() => {
    void loadResourceProfiles()
  }, [loadResourceProfiles])

  const tableHeaders: { label: string }[] = [
    { label: 'Agent size' },
    { label: 'Best for' },
    { label: 'Computer limit' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Agent Sizes</h2>
          <p className={uiStyles.sectionDescription}>
            Choose how much computer power an agent gets when it starts work.
          </p>
        </div>
      </div>

      {/* Error */}
      {resourceProfilesError && (
        <div className={uiStyles.error}>Could not load agent sizes. Try refreshing this page.</div>
      )}

      {/* Info note */}
      <div className={cn(uiStyles.note, 'mb-4')}>
        <p>
          Pick the smallest size that fits the job. Small sizes are cheaper and faster to start;
          larger sizes help when an agent needs to build, search, or inspect a bigger project.
        </p>
      </div>

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {resourceProfilesLoading && resourceProfiles.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading agent sizes...
          </div>
        ) : resourceProfiles.length === 0 ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No agent sizes available
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Ask an admin to add at least one default size before creating agents that work with
              files.
            </p>
          </div>
        ) : (
          <table className={uiStyles.table}>
            <thead className={uiStyles.tableHead}>
              <tr>
                {tableHeaders.map((h) => (
                  <th key={h.label} className={uiStyles.tableHeaderCell}>
                    {h.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {resourceProfiles.map((profile: ResourceProfileOption) => (
                <ProfileRow key={profile.id} profile={profile} />
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
