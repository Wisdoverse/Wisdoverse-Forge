import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { PreferenceGuideDisclosure, useSettingsStore } from '@app/entities/settings'

const originalSetGuideCollapsed = useSettingsStore.getState().setGuideCollapsed
const originalSetGuideDismissed = useSettingsStore.getState().setGuideDismissed
const setGuideCollapsed = vi.fn(async () => true)
const setGuideDismissed = vi.fn(async () => true)

beforeEach(() => {
  setGuideCollapsed.mockClear()
  setGuideDismissed.mockClear()
  useSettingsStore.setState({
    preferences: {},
    preferencesLoaded: true,
    preferencesLoading: false,
    setGuideCollapsed,
    setGuideDismissed,
  })
})

afterEach(() => {
  cleanup()
  useSettingsStore.setState({
    preferences: null,
    preferencesLoaded: false,
    preferencesLoading: false,
    setGuideCollapsed: originalSetGuideCollapsed,
    setGuideDismissed: originalSetGuideDismissed,
  })
})

describe('PreferenceGuideDisclosure', () => {
  test('starts collapsed for every account and persists open', () => {
    render(
      <PreferenceGuideDisclosure guideKey="agents-picker" icon={<svg />} title="Agent guide">
        Guide body
      </PreferenceGuideDisclosure>
    )

    const toggle = screen.getByRole('button', { name: 'Agent guide' })
    expect(toggle).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByText('Guide body')).toBeNull()

    fireEvent.click(toggle)

    expect(toggle).toHaveAttribute('aria-expanded', 'true')
    expect(screen.getByText('Guide body')).toBeDefined()
    expect(setGuideCollapsed).toHaveBeenCalledWith('agents-picker', false)
  })

  test('keeps the collapsed default for an established account and persists dismiss actions', () => {
    useSettingsStore.setState({ preferences: { gettingStartedDismissed: true } })

    render(
      <PreferenceGuideDisclosure guideKey="inbox-next-step" icon={<svg />} title="Do this next">
        Guide body
      </PreferenceGuideDisclosure>
    )

    const toggle = screen.getByRole('button', { name: 'Do this next' })
    expect(toggle).toHaveAttribute('aria-expanded', 'false')

    fireEvent.click(screen.getByRole('button', { name: 'Dismiss Do this next' }))

    expect(toggle).toHaveAttribute('aria-expanded', 'false')
    expect(setGuideDismissed).toHaveBeenCalledWith('inbox-next-step', true)
  })
})
