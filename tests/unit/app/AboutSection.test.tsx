import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { AboutSection } from '@app/features/settings/AboutSection'

afterEach(() => {
  cleanup()
})

describe('AboutSection', () => {
  test('keeps app help details short and beginner-readable', () => {
    render(<AboutSection />)

    const section = screen.getByTestId('settings-about')
    expect(within(section).getByRole('heading', { name: 'About Wisdoverse Forge' })).toBeDefined()
    expect(within(section).getByRole('heading', { name: 'App details' })).toBeDefined()
    expect(within(section).queryByText('Install details')).toBeNull()
    expect(
      within(section).queryByText(
        'Check what you are using before asking for help when something looks wrong.'
      )
    ).toBeNull()
    expect(within(section).queryByText(/reporting an issue/i)).toBeNull()
    expect(within(section).queryByText('Product name')).toBeNull()
    expect(within(section).queryByText(/sharing screenshots/i)).toBeNull()
    const legacySupportCopy = new RegExp(['asking for', 'support'].join(' '), 'i')
    expect(within(section).queryByText(legacySupportCopy)).toBeNull()
    expect(
      within(section).getByText('Share this number when something looks wrong after an update.')
    ).toBeDefined()
    const appDetails = within(section).getByText('Version').closest('dl')
    expect(appDetails).toHaveClass('border-y', 'bg-transparent')
    expect(appDetails?.className).not.toContain('rounded-card')
    expect(appDetails?.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(appDetails?.className).not.toContain('dark:bg-[#2c2c2e]')
    expect(screen.getByTestId('settings-about-version').textContent?.trim()).not.toBe('')
  })

  test('links to the project page without showing a raw repository URL as the label', () => {
    render(<AboutSection />)

    expect(
      screen.getByText('Open the public page for updates, fixes, and project details.')
    ).toBeDefined()
    expect(screen.queryByText(/releases, issues, and contribution details/i)).toBeNull()
    const link = screen.getByRole('link', { name: 'Open project page' })
    expect(link).toHaveAttribute('href', 'https://github.com/Wisdoverse/wisdoverse-forge')
    expect(link).toHaveAttribute('target', '_blank')
    expect(link.className).toContain('underline-offset-2')
    expect(link.className).not.toContain('text-apple-blue')
    expect(screen.queryByText('github.com/Wisdoverse/wisdoverse-forge')).toBeNull()
  })
})
