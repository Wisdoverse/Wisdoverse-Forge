import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useAdminStore, type AdminSection } from '@app/shared/model/admin.store'
import { UserManagement } from './UserManagement'
import { OrganizationsPanel } from './OrganizationsPanel'
import { SystemHealth } from './SystemHealth'

// ============================================================================
// Sidebar config
// ============================================================================

interface SectionItem {
  id: AdminSection
  label: string
  group: string
}

const SECTIONS: SectionItem[] = [
  { id: 'users', label: 'Users', group: 'Management' },
  { id: 'organizations', label: 'Organizations', group: 'Management' },
  { id: 'health', label: 'System Health', group: 'Infrastructure' },
]

const GROUPS = ['Management', 'Infrastructure']

// ============================================================================
// Content router
// ============================================================================

function SectionContent({ section }: { section: AdminSection }) {
  switch (section) {
    case 'users':
      return <UserManagement />
    case 'organizations':
      return <OrganizationsPanel />
    case 'health':
      return <SystemHealth />
    default:
      return null
  }
}

// ============================================================================
// AdminLayout
// ============================================================================

export function AdminLayout() {
  const { activeSection, setActiveSection } = useAdminStore()

  return (
    <div className="flex h-full flex-col overflow-hidden md:flex-row">
      <div className="shrink-0 border-b border-black/[0.06] px-4 py-3 dark:border-white/[0.06] md:hidden">
        <label htmlFor="admin-section-picker" className={uiStyles.label}>
          Section
        </label>
        <select
          id="admin-section-picker"
          value={activeSection}
          onChange={(event) => setActiveSection(event.target.value as AdminSection)}
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

      {/* Sidebar */}
      <nav
        className={cn(
          'hidden w-[200px] shrink-0 flex-col px-2 py-6 md:flex',
          'border-r border-black/[0.06] dark:border-white/[0.06]',
          'overflow-y-auto'
        )}
      >
        <h1 className="mb-4 px-2 text-ui-caption font-semibold uppercase text-secondary-light dark:text-secondary-dark">
          Admin
        </h1>

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
                    onClick={() => setActiveSection(item.id)}
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
      <main className="w-full max-w-5xl flex-1 overflow-y-auto px-4 py-5 sm:px-6 md:py-6">
        <SectionContent section={activeSection} />
      </main>
    </div>
  )
}
