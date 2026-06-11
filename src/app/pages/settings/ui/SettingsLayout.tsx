import { useEffect } from 'react'
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
  group: string
}

const SECTIONS: SectionItem[] = [
  { id: 'providers', label: 'AI Services', group: 'AI Setup' },
  { id: 'keys', label: 'Platform Access Keys', group: 'Work Setup' },
  { id: 'git-credentials', label: 'Code Repository Access', group: 'Work Setup' },
  { id: 'ssh-keys', label: 'SSH Access Keys', group: 'Work Setup' },
  { id: 'resources', label: 'Work Capacity', group: 'Work Setup' },
  { id: 'runtime', label: 'Agent Work Setup', group: 'Work Setup' },
  { id: 'account', label: 'Account', group: 'People' },
  { id: 'teams', label: 'Team Members', group: 'People' },
  { id: 'projects', label: 'Projects', group: 'People' },
  { id: 'about', label: 'About', group: 'Product Info' },
]

const GROUPS = ['AI Setup', 'Work Setup', 'People', 'Product Info']

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
          Section
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
      </div>

      {/* Desktop sidebar */}
      <nav
        data-testid="settings-desktop-nav"
        className={cn(
          'hidden md:flex w-[200px] shrink-0 flex-col py-6 px-2',
          'border-r border-black/[0.06] dark:border-white/[0.06]',
          'overflow-y-auto'
        )}
      >
        {GROUPS.map((group) => {
          const items = SECTIONS.filter((s) => s.group === group)
          return (
            <div key={group} className="mb-4">
              <p className="mb-1 px-2 text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
                {group}
              </p>
              {items.map((item) => {
                const isActive = activeSection === item.id
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => handleSectionChange(item.id)}
                    className={cn(
                      'w-full rounded-full px-3 py-1.5 text-left text-ui-button transition-colors',
                      isActive
                        ? cn('font-medium text-apple-blue', 'bg-apple-blue/10')
                        : 'text-foreground-light dark:text-foreground-dark hover:bg-black/5 dark:hover:bg-white/5'
                    )}
                  >
                    {item.label}
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
