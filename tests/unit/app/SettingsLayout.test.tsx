import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { SettingsLayout } from '@app/pages/settings/ui/SettingsLayout'
import { useSettingsStore } from '@app/shared/model/settings.store'

vi.mock('@app/features/settings', () => ({
  AboutSection: () => <div data-testid="settings-section-about">About settings</div>,
  AccountSection: () => <div data-testid="settings-section-account">Account settings</div>,
  GitCredentialsSection: () => (
    <div data-testid="settings-section-git-credentials">HTTPS code access settings</div>
  ),
  KeysSection: () => <div data-testid="settings-section-keys">Tool access keys settings</div>,
  ProvidersSection: () => <div data-testid="settings-section-providers">AI services settings</div>,
  ResourcesSection: () => (
    <div data-testid="settings-section-resources">Agent size limits settings</div>
  ),
  RuntimeSection: ({ focus }: { focus?: string }) => (
    <div data-testid="settings-section-runtime">{focus ?? 'Where agents work'} settings</div>
  ),
  SshKeysSection: () => <div data-testid="settings-section-ssh-keys">SSH code access settings</div>,
}))

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
  test('keeps advanced settings collapsed by default for beginner setup navigation', () => {
    const onSectionChange = vi.fn()

    render(<SettingsLayout routeSection="providers" onSectionChange={onSectionChange} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')
    const mobileNav = screen.getByTestId('settings-mobile-nav')

    expect(within(desktopNav).getByText('AI services')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Start here')).toBeInTheDocument()
    expect(within(desktopNav).queryByText('People and projects')).not.toBeInTheDocument()
    expect(within(desktopNav).queryByText('Access and limits')).not.toBeInTheDocument()
    expect(within(desktopNav).queryByText('Product info')).not.toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', { name: /Show team and project setup/i })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', { name: /Show advanced setup/i })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /AI services: Start here when agents need a chat service for answers and result checks/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).queryByRole('link', {
        name: /Projects: Create the work areas where tasks, agents, and files belong/i,
      })
    ).not.toBeInTheDocument()
    expect(
      within(desktopNav).queryByRole('link', {
        name: /Teams: Create teams and manage who can change work/i,
      })
    ).not.toBeInTheDocument()
    expect(
      within(desktopNav).queryByRole('link', {
        name: /Account: Update profile, password, and reset the setup checklist/i,
      })
    ).not.toBeInTheDocument()
    expect(
      within(desktopNav).queryByRole('link', {
        name: /Tool access keys: Create keys for trusted tools that need to connect to Forge/i,
      })
    ).not.toBeInTheDocument()
    expect(
      within(desktopNav).queryByRole('link', {
        name: /Code access for HTTPS links: Use this when your private code link starts with https:\/\//i,
      })
    ).not.toBeInTheDocument()
    expect(
      within(desktopNav).queryByRole('link', {
        name: /Code access for SSH links: Use this when your private code link starts with git@/i,
      })
    ).not.toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Where agents work: Choose Project files for the usual setup, or This computer for local-only work/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /File-change tool sign-in: Sign in before agents edit project files with Codex or another tool/i,
      })
    ).toBeInTheDocument()
    expect(within(desktopNav).queryByText('Team members')).not.toBeInTheDocument()
    expect(screen.queryByText(/Start guide reset/i)).toBeNull()

    expect(screen.getByRole('group', { name: 'Start here' })).toBeInTheDocument()
    expect(screen.queryByRole('group', { name: 'People and projects' })).not.toBeInTheDocument()
    expect(screen.queryByRole('group', { name: 'Access and limits' })).not.toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'AI services' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Projects' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Teams' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Account' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Tool access keys' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Outside tool access' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Outside apps' })).not.toBeInTheDocument()
    expect(
      screen.queryByRole('option', { name: 'Code access for HTTPS links' })
    ).not.toBeInTheDocument()
    expect(
      screen.queryByRole('option', { name: 'Code access for SSH links' })
    ).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'HTTPS code access' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'SSH code access' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Agent size limits' })).not.toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Where agents work' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'File-change tool sign-in' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Work tool sign-in' })).not.toBeInTheDocument()
    expect(
      screen.queryByRole('option', { name: 'Codex and work tool sign-in' })
    ).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Codex sign-in' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Codex CLI sign-in' })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: 'Team members' })).not.toBeInTheDocument()
    expect(screen.getByTestId('settings-mobile-section-hint')).toHaveTextContent(
      'Start here when agents need a chat service for answers and result checks.'
    )
    expect(
      within(mobileNav).getByRole('button', { name: /Show advanced setup/i })
    ).toBeInTheDocument()

    fireEvent.click(
      within(desktopNav).getByRole('link', {
        name: /Where agents work: Choose Project files for the usual setup, or This computer for local-only work/i,
      })
    )

    expect(onSectionChange).toHaveBeenCalledWith('runtime')

    fireEvent.click(
      within(desktopNav).getByRole('link', {
        name: /File-change tool sign-in: Sign in before agents edit project files with Codex or another tool/i,
      })
    )

    expect(onSectionChange).toHaveBeenCalledWith('work-tool-sign-ins')
  })

  test('reveals team and project Settings pages only after the user asks for them', () => {
    render(<SettingsLayout routeSection="providers" onSectionChange={vi.fn()} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')
    const mobileNav = screen.getByTestId('settings-mobile-nav')

    fireEvent.click(
      within(desktopNav).getByRole('button', { name: /Show team and project setup/i })
    )

    expect(within(desktopNav).getByText('People and projects')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Projects: Create the work areas where tasks, agents, and files belong/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', { name: /Hide team and project setup/i })
    ).toBeInTheDocument()

    expect(
      within(mobileNav).getByRole('button', { name: /Hide team and project setup/i })
    ).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'People and projects' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Projects' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Teams' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Account' })).toBeInTheDocument()
  })

  test('keeps team and project navigation open when the active route is a team setup page', () => {
    render(<SettingsLayout routeSection="account" onSectionChange={vi.fn()} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')

    expect(within(desktopNav).getByText('People and projects')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Account: Update profile, password, and reset the setup checklist/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', { name: /Hide team and project setup/i })
    ).toBeInTheDocument()
  })

  test('reveals advanced Settings pages only after the user asks for them', () => {
    render(<SettingsLayout routeSection="providers" onSectionChange={vi.fn()} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')

    fireEvent.click(within(desktopNav).getByRole('button', { name: /Show advanced setup/i }))

    expect(within(desktopNav).getByText('Access and limits')).toBeInTheDocument()
    expect(within(desktopNav).getByText('Product info')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Tool access keys: Create keys for trusted tools that need to connect to Forge/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /About: Check the app version and product details/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', { name: /Hide advanced setup/i })
    ).toBeInTheDocument()
  })

  test('keeps advanced navigation open when the active route is an advanced page', () => {
    render(<SettingsLayout routeSection="ssh-keys" onSectionChange={vi.fn()} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')

    expect(within(desktopNav).getByText('Access and limits')).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('link', {
        name: /Code access for SSH links: Use this when your private code link starts with git@/i,
      })
    ).toBeInTheDocument()
    expect(
      within(desktopNav).getByRole('button', { name: /Hide advanced setup/i })
    ).toBeInTheDocument()
  })

  test('desktop section navigation exposes direct links to each Settings page', () => {
    render(<SettingsLayout routeSection="about" onSectionChange={vi.fn()} />)

    const desktopNav = screen.getByTestId('settings-desktop-nav')

    fireEvent.click(
      within(desktopNav).getByRole('button', { name: /Show team and project setup/i })
    )

    expect(
      within(desktopNav).getByRole('link', {
        name: /Projects: Create the work areas where tasks, agents, and files belong/i,
      })
    ).toHaveAttribute('href', '/settings/projects')
    expect(
      within(desktopNav).getByRole('link', {
        name: /Where agents work: Choose Project files for the usual setup, or This computer for local-only work/i,
      })
    ).toHaveAttribute('href', '/settings/runtime')
    expect(
      within(desktopNav).getByRole('link', {
        name: /About: Check the app version and product details/i,
      })
    ).toHaveAttribute('href', '/settings/about')
  })
})
