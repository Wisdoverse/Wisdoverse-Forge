import { useState, type ReactNode } from 'react'
import { shouldShowGuide } from '@app/shared/lib/guidePreference'
import { GuideDisclosure } from '@app/shared/ui/GuideDisclosure'
import { useSettingsStore } from '../model/settings.store'

interface PreferenceGuideDisclosureProps {
  guideKey: string
  icon: ReactNode
  title: string
  children: ReactNode
  className?: string
  dismissible?: boolean
}

export function PreferenceGuideDisclosure({
  guideKey,
  icon,
  title,
  children,
  className,
  dismissible = true,
}: PreferenceGuideDisclosureProps) {
  const preferences = useSettingsStore((state) => state.preferences)
  const preferencesLoaded = useSettingsStore((state) => state.preferencesLoaded)
  const setGuideCollapsed = useSettingsStore((state) => state.setGuideCollapsed)
  const setGuideDismissed = useSettingsStore((state) => state.setGuideDismissed)

  if (!preferencesLoaded || !shouldShowGuide(guideKey, preferences)) return null

  return (
    <LoadedPreferenceGuideDisclosure
      guideKey={guideKey}
      icon={icon}
      title={title}
      className={className}
      dismissible={dismissible}
      onCollapsedChange={setGuideCollapsed}
      onDismiss={setGuideDismissed}
    >
      {children}
    </LoadedPreferenceGuideDisclosure>
  )
}

function LoadedPreferenceGuideDisclosure({
  guideKey,
  icon,
  title,
  children,
  className,
  dismissible,
  onCollapsedChange,
  onDismiss,
}: PreferenceGuideDisclosureProps & {
  onCollapsedChange: (key: string, collapsed: boolean) => Promise<boolean>
  onDismiss: (key: string, dismissed: boolean) => Promise<boolean>
}) {
  const [expanded, setExpanded] = useState(false)

  function handleToggle() {
    const nextExpanded = !expanded
    setExpanded(nextExpanded)
    void onCollapsedChange(guideKey, !nextExpanded)
  }

  return (
    <GuideDisclosure
      icon={icon}
      title={title}
      expanded={expanded}
      onToggle={handleToggle}
      onDismiss={dismissible ? () => void onDismiss(guideKey, true) : undefined}
      className={className}
    >
      {children}
    </GuideDisclosure>
  )
}
