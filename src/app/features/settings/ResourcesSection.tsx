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
          {profile.cpu} {profile.cpu === 1 ? 'core' : 'cores'}
        </span>
      </td>
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {formatMemory(profile.memoryMb)}
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
    { label: 'Profile' },
    { label: 'CPU' },
    { label: 'Memory' },
  ]

  return (
    <div>
      {/* Section header */}
      <div className={uiStyles.sectionHeader}>
        <div>
          <h2 className={uiStyles.sectionTitle}>Resource Profiles</h2>
          <p className={uiStyles.sectionDescription}>
            Available CPU and memory configurations for agent containers
          </p>
        </div>
      </div>

      {/* Error */}
      {resourceProfilesError && <div className={uiStyles.error}>{resourceProfilesError}</div>}

      {/* Info note */}
      <div className={cn(uiStyles.note, 'mb-4')}>
        <p>
          Resource profiles are defined by the platform administrator. Select a profile when
          creating an agent to control container resource limits.
        </p>
      </div>

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {resourceProfilesLoading && resourceProfiles.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading profiles...
          </div>
        ) : resourceProfiles.length === 0 ? (
          <div className="px-4 py-6 text-center">
            <p className="text-ui-body text-secondary-light dark:text-secondary-dark">
              No resource profiles available
            </p>
            <p className="mt-1 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Contact your administrator to configure resource profiles
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
