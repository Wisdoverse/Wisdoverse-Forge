import type { SettingsSection } from '@app/entities/settings'
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
