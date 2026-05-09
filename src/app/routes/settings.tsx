import { createRoute, redirect, useNavigate } from '@tanstack/react-router'
import { Route as rootRoute } from './__root'
import { SettingsLayout } from '@app/pages/settings'
import {
  SETTINGS_DEFAULT_SECTION,
  normalizeSettingsSection,
  type SettingsSection,
} from '@app/shared/model/settings.store'

export const Route = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: function SettingsIndexPage() {
    return <SettingsRoutePage section={SETTINGS_DEFAULT_SECTION} />
  },
})

export const SectionRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings/$section',
  beforeLoad: ({ params }) => {
    const section = normalizeSettingsSection(params.section)
    if (!section) {
      throw redirect({ to: '/settings' })
    }
    if (section !== params.section) {
      throw redirect({ to: '/settings/$section', params: { section } })
    }
  },
  component: function SettingsSectionPage() {
    const params = SectionRoute.useParams()
    return (
      <SettingsRoutePage
        section={normalizeSettingsSection(params.section) ?? SETTINGS_DEFAULT_SECTION}
      />
    )
  },
})

function SettingsRoutePage({ section }: { section: SettingsSection }) {
  const navigate = useNavigate()

  function handleSectionChange(nextSection: SettingsSection) {
    void navigate({ to: '/settings/$section', params: { section: nextSection } })
  }

  return (
    <div data-testid="page-settings" className="h-full">
      <SettingsLayout routeSection={section} onSectionChange={handleSectionChange} />
    </div>
  )
}
