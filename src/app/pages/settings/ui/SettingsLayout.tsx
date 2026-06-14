import { useEffect } from 'react'
import {
  Bot,
  Folder,
  Gauge,
  GitBranch,
  Info,
  Key,
  Settings2,
  Terminal,
  User,
  Users,
  type LucideIcon,
} from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useSettingsStore, type SettingsSection } from '@app/shared/model/settings.store'
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
    description: 'Connect the AI accounts agents use to think and write.',
    group: 'AI setup',
    Icon: Bot,
  },
  {
    id: 'keys',
    label: 'Outside apps',
    description: 'Add keys agents need to use apps and services outside Forge.',
    group: 'Work setup',
    Icon: Key,
  },
  {
    id: 'git-credentials',
    label: 'GitHub and GitLab access',
    description: 'Save HTTPS access for private GitHub or GitLab code.',
    group: 'Work setup',
    Icon: GitBranch,
  },
  {
    id: 'ssh-keys',
    label: 'SSH keys',
    description: 'Use this when a private code link starts with git@.',
    group: 'Work setup',
    Icon: Terminal,
  },
  {
    id: 'resources',
    label: 'Work limits',
    description: 'Choose safe small, standard, or large work limits.',
    group: 'Work setup',
    Icon: Gauge,
  },
  {
    id: 'runtime',
    label: 'Where agents run',
    description: 'Choose where agents run and which work tool they use.',
    group: 'Work setup',
    Icon: Settings2,
  },
  {
    id: 'account',
    label: 'Account',
    description: 'Update profile, password, and show the setup checklist again.',
    group: 'People',
    Icon: User,
  },
  {
    id: 'teams',
    label: 'Team members',
    description: 'Invite people and manage who can change work.',
    group: 'People',
    Icon: Users,
  },
  {
    id: 'projects',
    label: 'Projects',
    description: 'Create the work areas agents use for tasks and files.',
    group: 'People',
    Icon: Folder,
  },
  {
    id: 'about',
    label: 'About',
    description: 'Check version and product information.',
    group: 'Product info',
    Icon: Info,
  },
]

const GROUPS = ['AI setup', 'Work setup', 'People', 'Product info']
const SECTION_BY_ID = new Map(SECTIONS.map((section) => [section.id, section]))

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

  useEffect(() => {
    if (routeSection && storedActiveSection !== routeSection) {
      setActiveSection(routeSection)
    }
  }, [routeSection, setActiveSection, storedActiveSection])

  function handleSectionChange(section: SettingsSection) {
    setActiveSection(section)
    onSectionChange?.(section)
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
          {GROUPS.map((group) => (
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
          className="mt-2 rounded-lg bg-black/[0.025] px-3 py-2 text-ui-caption text-secondary-light dark:bg-white/[0.035] dark:text-secondary-dark"
        >
          {activeSectionItem.description}
        </p>
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
        {GROUPS.map((group) => {
          const items = SECTIONS.filter((s) => s.group === group)
          return (
            <div key={group} className="mb-4">
              <p className="mb-1 px-2 text-ui-caption font-semibold text-secondary-light dark:text-secondary-dark">
                {group}
              </p>
              {items.map((item) => {
                const isActive = activeSection === item.id
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => handleSectionChange(item.id)}
                    aria-label={`${item.label}: ${item.description}`}
                    className={cn(
                      'flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors',
                      isActive
                        ? cn('text-apple-blue', 'bg-apple-blue/10')
                        : 'text-foreground-light dark:text-foreground-dark hover:bg-black/5 dark:hover:bg-white/5'
                    )}
                  >
                    <item.Icon
                      size={15}
                      strokeWidth={2.2}
                      className="mt-0.5 shrink-0"
                      aria-hidden="true"
                    />
                    <span className="min-w-0">
                      <span className="block text-ui-button font-medium">{item.label}</span>
                      <span
                        className={cn(
                          'mt-0.5 block text-ui-caption leading-snug',
                          isActive
                            ? 'text-apple-blue/85'
                            : 'text-secondary-light dark:text-secondary-dark'
                        )}
                      >
                        {item.description}
                      </span>
                    </span>
                  </button>
                )
              })}
            </div>
          )
        })}
      </nav>

      {/* Content area */}
      <main className="w-full max-w-4xl flex-1 overflow-y-auto px-4 py-5 sm:px-6 md:py-6">
        <SectionContent section={activeSection} />
      </main>
    </div>
  )
}
