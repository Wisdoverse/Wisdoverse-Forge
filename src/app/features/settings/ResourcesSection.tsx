import { useEffect } from 'react'
import { Cpu, HardDrive, ShieldCheck, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/shared/model/settings.store'
import type { ResourceProfileOption } from '@app/entities/agent'

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

function resourceProfileUseCase(profile: ResourceProfileOption): string {
  if (profile.cpu <= 1 && profile.memoryMb <= 2048) {
    return 'Light reviews, docs, and short commands'
  }
  if (profile.cpu <= 2 && profile.memoryMb <= 4096) {
    return 'Normal coding tasks and test runs'
  }
  return 'Large builds, browser tests, and long-running work'
}

function summarizeProfileRange(profiles: ResourceProfileOption[]): string {
  if (profiles.length === 0) {
    return 'Ask an administrator to add at least one profile before creating container agents.'
  }
  const sorted = [...profiles].sort((a, b) => a.cpu - b.cpu || a.memoryMb - b.memoryMb)
  const smallest = sorted[0]
  const largest = sorted[sorted.length - 1]
  if (smallest.id === largest.id) {
    return `${smallest.name} is available with ${smallest.cpu} ${smallest.cpu === 1 ? 'core' : 'cores'} and ${formatMemory(
      smallest.memoryMb
    )} memory.`
  }
  return `${smallest.name} is the smallest option; ${largest.name} is the largest option.`
}

const RESOURCE_PROFILE_GUIDANCE: {
  title: string
  description: string
  Icon: LucideIcon
}[] = [
  {
    title: 'CPU controls speed',
    description: 'More cores help builds, tests, and code search finish faster.',
    Icon: Cpu,
  },
  {
    title: 'Memory prevents crashes',
    description: 'More memory helps large repositories, browser tests, and compilers stay stable.',
    Icon: HardDrive,
  },
  {
    title: 'Limits protect the runner',
    description: 'Profiles keep one agent from consuming all machine resources.',
    Icon: ShieldCheck,
  },
]

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
      <td className={uiStyles.tableCell}>
        <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
          {resourceProfileUseCase(profile)}
        </span>
      </td>
    </tr>
  )
}

function ResourceProfileGuide({ profiles }: { profiles: ResourceProfileOption[] }) {
  return (
    <section
      data-testid="resource-profile-guide"
      className={cn(
        'mb-4 rounded-card border border-black/[0.08] bg-white p-4 dark:border-white/[0.1] dark:bg-[#2c2c2e]'
      )}
    >
      <div className="mb-3">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Before choosing a profile
        </p>
        <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Pick the smallest profile that can finish the work
        </h3>
        <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          {summarizeProfileRange(profiles)}
        </p>
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        {RESOURCE_PROFILE_GUIDANCE.map(({ title, description, Icon }) => (
          <div key={title} className="rounded-lg bg-black/[0.03] p-3 dark:bg-white/[0.04]">
            <div className="mb-2 flex items-center gap-2 text-foreground-light dark:text-foreground-dark">
              <Icon size={14} strokeWidth={2.2} className="shrink-0 text-apple-blue" />
              <p className="text-ui-caption font-semibold">{title}</p>
            </div>
            <p className="text-ui-caption text-secondary-light dark:text-secondary-dark">
              {description}
            </p>
          </div>
        ))}
      </div>
    </section>
  )
}

function ResourceProfilesEmptyState() {
  return (
    <div data-testid="resource-profiles-empty" className="px-4 py-6 text-center">
      <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
        No resource profiles are available yet
      </p>
      <p className="mx-auto mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
        Agents need at least one profile before operators can choose CPU and memory limits.
      </p>
      <div className="mx-auto mt-4 grid max-w-2xl gap-2 text-left sm:grid-cols-3">
        <p className="rounded-lg bg-black/[0.03] p-3 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          Ask an administrator to add profiles in platform configuration.
        </p>
        <p className="rounded-lg bg-black/[0.03] p-3 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          Start with small, standard, and large options so users can choose safely.
        </p>
        <p className="rounded-lg bg-black/[0.03] p-3 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          Return here before creating container agents; at least one row means this step is ready.
        </p>
      </div>
    </div>
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
    { label: 'Best for' },
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

      <ResourceProfileGuide profiles={resourceProfiles} />

      {/* Table */}
      <div className={cn(uiStyles.card, 'overflow-x-auto')}>
        {resourceProfilesLoading && resourceProfiles.length === 0 ? (
          <div className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
            Loading agent sizes...
          </div>
        ) : resourceProfiles.length === 0 ? (
          <ResourceProfilesEmptyState />
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
