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
    expect(within(desktopNav).getByText('AI setup')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Work setup')).toBeInTheDocument()
    expect(within(desktopNav).getByText('People')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Product info')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /AI services: Connect the AI accounts agents use to think and write/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Outside apps: Add keys agents need to use apps and services outside Forge/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /GitHub and GitLab access: Save HTTPS access for private GitHub or GitLab code/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /SSH keys: Use this when a private code link starts with git@/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Where agents run: Choose where agents run and which work tool they use/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Account: Update profile, password, and show the setup checklist again/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', {
        name: /Teams: Create teams, invite people, and manage who can change work/i,
      })
    ).toBeInTheDocument()
    expect(within(desktopNav).queryByText('Team members')).not.toBeInTheDocument()
    expect(screen.queryByText(/Start guide reset/i)).toBeNull()

    expect(screen.getByRole('group', { name: 'AI setup' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'Work setup' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'People' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'AI services' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Outside apps' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'GitHub and GitLab access' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'SSH keys' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Work limits' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Teams' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Team members' })).not.toBeInTheDocument()
    expect(screen.getByTestId('settings-mobile-section-hint')).toHaveTextContent(
      'Check version and product information.'
    )

    fireEvent.click(
      within(desktopNav).getByRole('button', {
        name: /Where agents run: Choose where agents run and which work tool they use/i,
      })
    )

    expect(onSectionChange).toHaveBeenCalledWith('runtime')
  })
})
