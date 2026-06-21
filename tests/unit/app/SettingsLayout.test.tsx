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
    expect(within(desktopNav).getAllByText('AI services').length).toBeGreaterThanOrEqual(2)
    expect(within(desktopNav).getByText('Agent work')).toBeInTheDocument()
    expect(within(desktopNav).getByText('People')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Product info')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /AI services: Connect the AI accounts agents use to think and write/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Outside tool access: Let trusted outside tools connect to Forge without a person signing in/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /HTTPS code access: Use this when a private code link starts with https:\/\//i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /SSH code access: Use this when a private code link starts with git@/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Where agents work: Choose where project files open and which work tool agents use/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Codex and work tool sign-in: Sign in to the account Codex uses and other work tools used for file work/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Account: Update profile, password, and show the setup checklist again/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Teams: Create teams, invite people, and manage who can change work/i,
      })
    ).toBeInTheDocument()
    expect(within(desktopNav).queryByText('Team members')).not.toBeInTheDocument()
    expect(screen.queryByText(/Start guide reset/i)).toBeNull()

    expect(screen.getByRole('group', { name: 'AI services' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'Agent work' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'People' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'AI services' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Outside tool access' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Outside apps' })).not.toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'HTTPS code access' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'SSH code access' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Agent size limits' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Where agents work' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Codex and work tool sign-in' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Codex CLI sign-in' })).not.toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Teams' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Team members' })).not.toBeInTheDocument()
    expect(screen.getByTestId('settings-mobile-section-hint')).toHaveTextContent(
      'Check the app version and product details.'
    )

    fireEvent.click(
      within(desktopNav).getByRole('link', {
        name: /Where agents work: Choose where project files open and which work tool agents use/i,
      })
    )

    expect(onSectionChange).toHaveBeenCalledWith('runtime')

    fireEvent.click(
      within(desktopNav).getByRole('link', {
        name: /Codex and work tool sign-in: Sign in to the account Codex uses and other work tools used for file work/i,
      })
    )

    expect(onSectionChange).toHaveBeenCalledWith('work-tool-sign-ins')
  })

  test('desktop section navigation exposes direct links to each Settings page', () => {
    render(<SettingsLayout routeSection="about" onSectionChange={vi.fn()} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')

    expect(
      within(desktopNav).getByRole('link', {
        name: /Projects: Create the work areas agents use for tasks and files/i,
      })
    ).toHaveAttribute('href', '/settings/projects')
    expect(
      within(desktopNav).getByRole('link', {
        name: /Where agents work: Choose where project files open and which work tool agents use/i,
      })
    ).toHaveAttribute('href', '/settings/runtime')
  })
})
