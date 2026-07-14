import { type MouseEvent, useEffect, useState } from 'react'
import {
  Bot,
  ChevronDown,
  ChevronRight,
  Folder,
  Gauge,
  GitBranch,
  Info,
  Key,
  LogIn,
  Settings2,
  Terminal,
  User,
  Users,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore, type SettingsSection } from '@app/entities/settings'
import {
  AboutSection,
  AccountSection,
  GitCredentialsSection,
  KeysSection,
  ProvidersSection,
  ResourcesSection,
  RuntimeSection,
  SshKeysSection,
} from '@app/features/settings'
import { TeamsSection } from './TeamsSection'
import { ProjectsSection } from './ProjectsSection'

// ============================================================================
// Sidebar config
// ============================================================================

interface SectionItem {
  id: SettingsSection
  label: string
  description: string
  group: string
  Icon: LucideIcon
}

const SECTIONS: SectionItem[] = [
  {
    id: 'providers',
    label: 'AI services',
    description: 'Start here when agents need a chat service for answers and result checks.',
    group: 'Start here',
    Icon: Bot,
  },
  {
    id: 'runtime',
    label: 'Where agents work',
    description: 'Choose Project files for the usual setup, or This computer for local-only work.',
    group: 'Start here',
    Icon: Settings2,
  },
  {
    id: 'work-tool-sign-ins',
    label: 'Sign in to code tools',
    description: 'Sign in before agents edit project files with Codex or another tool.',
    group: 'Start here',
    Icon: LogIn,
  },
  {
    id: 'projects',
    label: 'Projects',
    description: 'Create the work areas where tasks, agents, and files belong.',
    group: 'People and projects',
    Icon: Folder,
  },
  {
    id: 'teams',
    label: 'Teams',
    description: 'Create teams and manage who can change work.',
    group: 'People and projects',
    Icon: Users,
  },
  {
    id: 'account',
    label: 'Account',
    description: 'Update profile, password, and reset the setup checklist.',
    group: 'People and projects',
    Icon: User,
  },
  {
    id: 'git-credentials',
    label: 'Code access for HTTPS links',
    description: 'Use this when your private code link starts with https://.',
    group: 'Access and limits',
    Icon: GitBranch,
  },
  {
    id: 'ssh-keys',
    label: 'Code access for SSH links',
    description: 'Use this when your private code link starts with git@.',
    group: 'Access and limits',
    Icon: Terminal,
  },
  {
    id: 'keys',
    label: 'Tool access keys',
    description: 'Create keys for trusted tools that need to connect to Forge.',
    group: 'Access and limits',
    Icon: Key,
  },
  {
    id: 'resources',
    label: 'Agent size limits',
    description: 'Choose small, standard, or large limits before agents change project files.',
    group: 'Access and limits',
    Icon: Gauge,
  },
  {
    id: 'about',
    label: 'About',
    description: 'Check the app version and product details.',
    group: 'Product info',
    Icon: Info,
  },
]

const PRIMARY_GROUPS = ['Start here'] as const
const TEAM_PROJECT_GROUPS = ['People and projects'] as const
const ADVANCED_GROUPS = ['Access and limits', 'Product info'] as const
const SECTION_BY_ID = new Map(SECTIONS.map((section) => [section.id, section]))

function isTeamProjectGroup(group: string): boolean {
  return (TEAM_PROJECT_GROUPS as readonly string[]).includes(group)
}

function isAdvancedGroup(group: string): boolean {
  return (ADVANCED_GROUPS as readonly string[]).includes(group)
}

// ============================================================================
// Content router
// ============================================================================

function SectionContent({ section }: { section: SettingsSection }) {
  switch (section) {
    case 'providers':
      return <ProvidersSection />
    case 'keys':
      return <KeysSection />
    case 'account':
      return <AccountSection />
    case 'git-credentials':
      return <GitCredentialsSection />
    case 'ssh-keys':
      return <SshKeysSection />
    case 'resources':
      return <ResourcesSection />
    case 'runtime':
      return <RuntimeSection />
    case 'work-tool-sign-ins':
      return <RuntimeSection focus="sign-ins" />
    case 'teams':
      return <TeamsSection />
    case 'projects':
      return <ProjectsSection />
    case 'about':
      return <AboutSection />
    default:
      return null
  }
}

// ============================================================================
// SettingsLayout
// ============================================================================

interface SettingsLayoutProps {
  routeSection?: SettingsSection
  onSectionChange?: (section: SettingsSection) => void
}

export function SettingsLayout({ routeSection, onSectionChange }: SettingsLayoutProps = {}) {
  const { activeSection: storedActiveSection, setActiveSection } = useSettingsStore()
  const activeSection = routeSection ?? storedActiveSection
  const activeSectionItem = SECTION_BY_ID.get(activeSection) ?? SECTIONS[0]
  const activeSectionIsTeamProject = isTeamProjectGroup(activeSectionItem.group)
  const activeSectionIsAdvanced = isAdvancedGroup(activeSectionItem.group)
  const [teamProjectOpen, setTeamProjectOpen] = useState(activeSectionIsTeamProject)
  const [advancedOpen, setAdvancedOpen] = useState(activeSectionIsAdvanced)
  const visibleMobileGroups = [
    ...PRIMARY_GROUPS,
    ...(teamProjectOpen ? TEAM_PROJECT_GROUPS : []),
    ...(advancedOpen ? ADVANCED_GROUPS : []),
  ]

  useEffect(() => {
    if (routeSection && storedActiveSection !== routeSection) {
      setActiveSection(routeSection)
    }
  }, [routeSection, setActiveSection, storedActiveSection])

  useEffect(() => {
    if (activeSectionIsTeamProject) {
      setTeamProjectOpen(true)
    }
    if (activeSectionIsAdvanced) {
      setAdvancedOpen(true)
    }
  }, [activeSectionIsAdvanced, activeSectionIsTeamProject])

  function handleSectionChange(section: SettingsSection) {
    setActiveSection(section)
    onSectionChange?.(section)
  }

  function handleSectionLinkClick(event: MouseEvent<HTMLAnchorElement>, section: SettingsSection) {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.altKey ||
      event.ctrlKey ||
      event.shiftKey
    ) {
      return
    }

    event.preventDefault()
    handleSectionChange(section)
  }

  return (
    <div className="flex h-full flex-col overflow-hidden md:flex-row">
      {/* Mobile-only: grouped section picker at top */}
      <div
        data-testid="settings-mobile-nav"
        className="shrink-0 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06] md:hidden"
      >
        <label htmlFor="settings-section-picker" className={uiStyles.label}>
          What do you want to set up?
        </label>
        <select
          id="settings-section-picker"
          value={activeSection}
          onChange={(e) => handleSectionChange(e.target.value as SettingsSection)}
          className={cn(uiStyles.select, 'w-full')}
        >
          {visibleMobileGroups.map((group) => (
            <optgroup key={group} label={group}>
              {SECTIONS.filter((s) => s.group === group).map((item) => (
                <option key={item.id} value={item.id}>
                  {item.label}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
        <p
          data-testid="settings-mobile-section-hint"
          className="mt-2 rounded-card bg-black/[0.025] px-3 py-2 text-ui-caption text-secondary-light dark:bg-white/[0.035] dark:text-secondary-dark"
        >
          {activeSectionItem.description}
        </p>
        <SettingsDisclosureButton
          open={teamProjectOpen}
          openLabel="Hide team and project setup"
          closedLabel="Show team and project setup"
          onClick={() => setTeamProjectOpen((open) => !open)}
          className="mt-2"
        />
        <SettingsDisclosureButton
          open={advancedOpen}
          openLabel="Hide more setup"
          closedLabel="Show more setup"
          onClick={() => setAdvancedOpen((open) => !open)}
          className="mt-2"
        />
      </div>

      {/* Desktop sidebar */}
      <nav
        data-testid="settings-desktop-nav"
        className={cn(
          'hidden md:flex w-[248px] shrink-0 flex-col py-6 px-2',
          'border-r border-black/[0.06] dark:border-white/[0.06]',
          'overflow-y-auto'
        )}
      >
        {PRIMARY_GROUPS.map((group) => (
          <SettingsSectionGroup
            key={group}
            group={group}
            activeSection={activeSection}
            onSectionLinkClick={handleSectionLinkClick}
          />
        ))}
        <SettingsDisclosureButton
          open={teamProjectOpen}
          openLabel="Hide team and project setup"
          closedLabel="Show team and project setup"
          onClick={() => setTeamProjectOpen((open) => !open)}
          className="mx-2 mb-4"
        />
        {teamProjectOpen &&
          TEAM_PROJECT_GROUPS.map((group) => (
            <SettingsSectionGroup
              key={group}
              group={group}
              activeSection={activeSection}
              onSectionLinkClick={handleSectionLinkClick}
            />
          ))}
        <SettingsDisclosureButton
          open={advancedOpen}
          openLabel="Hide more setup"
          closedLabel="Show more setup"
          onClick={() => setAdvancedOpen((open) => !open)}
          className="mx-2 mb-4"
        />
        {advancedOpen &&
          ADVANCED_GROUPS.map((group) => (
            <SettingsSectionGroup
              key={group}
              group={group}
              activeSection={activeSection}
              onSectionLinkClick={handleSectionLinkClick}
            />
          ))}
      </nav>

      {/* Content area */}
      <main className="w-full max-w-4xl flex-1 overflow-y-auto px-4 py-5 sm:px-6 md:py-6">
        <SectionContent section={activeSection} />
      </main>
    </div>
  )
}

function SettingsSectionGroup({
  group,
  activeSection,
  onSectionLinkClick,
}: {
  group: string
  activeSection: SettingsSection
  onSectionLinkClick: (event: MouseEvent<HTMLAnchorElement>, section: SettingsSection) => void
}) {
  const items = SECTIONS.filter((section) => section.group === group)

  return (
    <div className="mb-4">
      <p className={cn(uiStyles.groupLabel, 'mb-1 px-2')}>{group}</p>
      {items.map((item) => {
        const isActive = activeSection === item.id
        return (
          <a
            key={item.id}
            href={`/settings/${item.id}`}
            onClick={(event) => onSectionLinkClick(event, item.id)}
            aria-label={`${item.label}: ${item.description}`}
            className={cn(
              'flex w-full items-start gap-2 rounded-button px-2.5 py-2 text-left transition-colors',
              isActive
                ? 'bg-black/[0.06] text-foreground-light dark:bg-white/[0.08] dark:text-foreground-dark'
                : 'text-foreground-light hover:bg-black/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.06]'
            )}
          >
            <item.Icon size={16} strokeWidth={2.2} className="mt-0.5 shrink-0" aria-hidden="true" />
            <span className="min-w-0">
              <span className="block text-ui-button font-medium">{item.label}</span>
              <span
                className={cn(
                  'mt-0.5 block text-ui-caption leading-snug',
                  'text-secondary-light dark:text-secondary-dark'
                )}
              >
                {item.description}
              </span>
            </span>
          </a>
        )
      })}
    </div>
  )
}

function SettingsDisclosureButton({
  open,
  openLabel,
  closedLabel,
  onClick,
  className,
}: {
  open: boolean
  openLabel: string
  closedLabel: string
  onClick: () => void
  className?: string
}) {
  const Icon = open ? ChevronDown : ChevronRight

  return (
    <button
      type="button"
      aria-expanded={open}
      onClick={onClick}
      className={cn(
        'inline-flex min-h-8 w-full items-center justify-between gap-2 rounded-button border border-black/[0.06] bg-black/[0.02] px-2.5 py-1.5 text-left text-ui-button font-medium text-foreground-light transition-colors hover:bg-black/[0.04] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus dark:border-white/[0.08] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.06]',
        className
      )}
    >
      <span>{open ? openLabel : closedLabel}</span>
      <Icon
        size={15}
        strokeWidth={2.2}
        className="shrink-0 text-secondary-light dark:text-secondary-dark"
        aria-hidden="true"
      />
    </button>
  )
}
