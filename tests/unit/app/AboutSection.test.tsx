import { afterEach, describe, expect, test } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { AboutSection } from '@app/features/settings/AboutSection'

afterEach(() => {
  cleanup()
})

describe('AboutSection', () => {
  test('explains install details in beginner help language', () => {
    render(<AboutSection />)

    const section = screen.getByTestId('settings-about')
    expect(within(section).getByRole('heading', { name: 'About Wisdoverse Forge' })).toBeDefined()
    expect(
      within(section).getByText(
        'Check what you are using before asking for help or reporting an issue.'
      )
    ).toBeDefined()
    expect(within(section).getByText('Product name')).toBeDefined()
    expect(
      within(section).getByText(
        'Use this name when sharing screenshots or asking an owner or admin for help.'
      )
    ).toBeDefined()
    const legacySupportCopy = new RegExp(['asking for', 'support'].join(' '), 'i')
    expect(within(section).queryByText(legacySupportCopy)).toBeNull()
    expect(
      within(section).getByText('Share this number when something looks wrong after an update.')
    ).toBeDefined()
    expect(screen.getByTestId('settings-about-version').textContent?.trim()).not.toBe('')
  })

  test('links to the project page without showing a raw repository URL as the label', () => {
    render(<AboutSection />)

    const link = screen.getByRole('link', { name: 'Open project page' })
    expect(link).toHaveAttribute('href', 'https://github.com/Wisdoverse/wisdoverse-forge')
    expect(link).toHaveAttribute('target', '_blank')
    expect(screen.queryByText('github.com/Wisdoverse/wisdoverse-forge')).toBeNull()
  })
})
