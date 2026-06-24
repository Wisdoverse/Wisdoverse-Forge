import type { SettingsSection } from '@app/shared/model/settings.store'
import { SettingsLayout } from './SettingsLayout'

interface SettingsPageProps {
  section: SettingsSection
  onSectionChange: (section: SettingsSection) => void
}

export function SettingsPage({ section, onSectionChange }: SettingsPageProps) {
  return (
    <div data-testid="page-settings" className="h-full">
      <SettingsLayout routeSection={section} onSectionChange={onSectionChange} />
    </div>
  )
}
