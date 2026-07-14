import { useEffect } from 'react'
import { Cpu, HardDrive, ShieldCheck, type LucideIcon } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore } from '@app/entities/settings'
import { BeginnerLoadingState } from '@app/shared/ui/BeginnerLoadingState'
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
  return `${cpu} processing ${cpu === 1 ? 'core' : 'cores'}`
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
    return 'Ask an owner or admin to add at least one agent size before creating agents that edit project files.'
  }
  const sorted = [...profiles].sort((a, b) => a.cpu - b.cpu || a.memoryMb - b.memoryMb)
  const smallest = sorted[0]
  const largest = sorted[sorted.length - 1]
  if (smallest.id === largest.id) {
    return `${smallest.name} is available with ${formatCpu(smallest.cpu)} and ${formatMemory(
      smallest.memoryMb
    )} memory.`
  }
  return `${smallest.name} is the smallest size; ${largest.name} is the largest size.`
}

const RESOURCE_PROFILE_GUIDANCE: {
  title: string
  description: string
  Icon: LucideIcon
}[] = [
  {
    title: 'More processing power speeds work up',
    description: 'More processing cores help builds, tests, and code search finish faster.',
    Icon: Cpu,
  },
  {
    title: 'More memory keeps large work stable',
    description: 'More memory helps large repositories, browser tests, and compilers stay stable.',
    Icon: HardDrive,
  },
  {
    title: 'Sizes keep work fair for everyone',
    description: 'They keep one agent from using all shared work capacity.',
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
          {formatCpu(profile.cpu)} · {formatMemory(profile.memoryMb)} memory
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
      className="mb-4 border-y border-black/[0.06] bg-transparent py-4 dark:border-white/[0.08]"
    >
      <div className="mb-3">
        <p className="text-ui-caption font-medium text-secondary-light dark:text-secondary-dark">
          Before choosing a size
        </p>
        <h3 className="text-ui-section font-semibold text-foreground-light dark:text-foreground-dark">
          Pick the smallest size that can finish the work
        </h3>
        <p className="mt-1 text-ui-body text-secondary-light dark:text-secondary-dark">
          {summarizeProfileRange(profiles)}
        </p>
      </div>
      <div className="grid gap-2 md:grid-cols-3">
        {RESOURCE_PROFILE_GUIDANCE.map(({ title, description, Icon }) => (
          <div key={title} className="rounded-card bg-black/[0.03] p-3 dark:bg-white/[0.04]">
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
        Ask an owner or admin to add agent sizes
      </p>
      <p className="mx-auto mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
        Agents need at least one size before users can choose safe work capacity.
      </p>
      <p className="mx-auto mt-3 max-w-xl text-ui-caption font-medium text-foreground-light dark:text-foreground-dark">
        Next step: ask an owner or admin to open Work limits and add one agent size.
      </p>
      <p className="mx-auto mt-1 max-w-xl text-ui-caption text-secondary-light dark:text-secondary-dark">
        Success looks like one size listed here, such as Small or Standard.
      </p>
      <div className="mx-auto mt-4 grid max-w-2xl gap-2 text-left sm:grid-cols-3">
        <p className="rounded-card bg-black/[0.03] p-3 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          Ask an owner or admin to add agent sizes in Work limits.
        </p>
        <p className="rounded-card bg-black/[0.03] p-3 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          Start with small, standard, and large sizes so users can choose safely.
        </p>
        <p className="rounded-card bg-black/[0.03] p-3 text-ui-caption text-secondary-light dark:bg-white/[0.04] dark:text-secondary-dark">
          Return here before creating agents that edit project files; at least one row means this
          step is ready.
        </p>
      </div>
    </div>
  )
}

function ResourceProfilesError({
  loading,
  onRetry,
}: {
  loading: boolean
  onRetry: () => Promise<void>
}) {
  return (
    <div role="alert" aria-live="polite" className={cn(uiStyles.error, 'mb-4')}>
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <p className="font-semibold">Check agent sizes again.</p>
          <p className="mt-1">
            Agent sizes decide how much computer power and memory an agent that edits project files
            can use. Check this list again before creating or changing agents that edit project
            files.
          </p>
        </div>
        <button
          type="button"
          onClick={() => void onRetry()}
          disabled={loading}
          className={cn(
            uiStyles.secondaryButton,
            'shrink-0 border-apple-red/30 text-apple-red dark:border-apple-red/30 dark:text-apple-red'
          )}
        >
          {loading ? 'Checking agent sizes' : 'Check again'}
        </button>
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
    { label: 'Size' },
    { label: 'Good fit' },
    { label: 'Power and memory' },
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
        <ResourceProfilesError loading={resourceProfilesLoading} onRetry={loadResourceProfiles} />
      )}

      <ResourceProfileGuide profiles={resourceProfiles} />

      {/* Table */}
      <div className="overflow-x-auto border-y border-black/[0.06] bg-transparent dark:border-white/[0.08]">
        {resourceProfilesLoading && resourceProfiles.length === 0 ? (
          <BeginnerLoadingState
            framed={false}
            title="Checking agent sizes"
            detail="Forge is checking which agent sizes are available for this team space."
            nextStep="If this takes more than a moment, open Agent Sizes again or ask an owner or admin to check Work limits."
            success="Success looks like a size row with power, memory, and a best-fit use case."
          />
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
