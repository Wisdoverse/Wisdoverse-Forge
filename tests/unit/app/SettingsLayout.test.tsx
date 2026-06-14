import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { SettingsLayout } from '@app/pages/settings/ui/SettingsLayout'
import { useSettingsStore } from '@app/shared/model/settings.store'

const originalActiveSection = useSettingsStore.getState().activeSection

beforeEach(() => {
  vi.stubGlobal('__APP_VERSION__', 'test-version')
  useSettingsStore.setState({ activeSection: 'about' })
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  useSettingsStore.setState({ activeSection: originalActiveSection })
})

describe('SettingsLayout', () => {
  test('uses task-first setup labels in desktop and mobile navigation', () => {
    const onSectionChange = vi.fn()

    render(<SettingsLayout routeSection="about" onSectionChange={onSectionChange} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')
    expect(within(desktopNav).getByText('AI Setup')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Work Setup')).toBeInTheDocument()
    expect(within(desktopNav).getByText('People')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Product Info')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /AI Services: Connect the AI accounts agents use to think and write/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Outside Tool Access: Add keys agents need for outside apps and services/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Code Access: Save HTTPS access for private GitHub or GitLab code/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /SSH Code Access: Use this when a private code link starts with git@/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Agent Work Setup: Choose where agents run and which work tool they use/i,
      })
    ).toBeInTheDocument()

    expect(screen.getByRole('group', { name: 'AI Setup' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'Work Setup' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'AI Services' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Outside Tool Access' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Code Access' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'SSH Code Access' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Work Capacity' })).toBeInTheDocument()
    expect(screen.getByTestId('settings-mobile-section-hint')).toHaveTextContent(
      'Check version and product information.'
    )

    fireEvent.click(
      within(desktopNav).getByRole('button', {
        name: /Agent Work Setup: Choose where agents run and which work tool they use/i,
      })
    )

    expect(onSectionChange).toHaveBeenCalledWith('runtime')
  })
})
