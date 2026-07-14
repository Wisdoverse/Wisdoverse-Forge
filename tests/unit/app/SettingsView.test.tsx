import { render, screen } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'
import { SettingsView } from '@app/features/settings/SettingsView'
import { ThemeContext } from '@app/shared/model/theme.context'

function renderSettingsView() {
  return render(
    <ThemeContext.Provider value={{ theme: 'light', toggleTheme: vi.fn(), setTheme: vi.fn() }}>
      <SettingsView />
    </ThemeContext.Provider>
  )
}

describe('SettingsView', () => {
  test('keeps overview sections lightweight instead of card framed', () => {
    renderSettingsView()

    expect(screen.getByRole('heading', { name: 'Display' })).toBeDefined()
    expect(screen.queryByRole('heading', { name: 'Appearance' })).toBeNull()
    const displayFrame = screen.getByText('Theme').closest('div')!.parentElement!
    expect(displayFrame).toHaveClass('border-y', 'bg-transparent')
    expect(displayFrame.className).not.toContain('rounded-card')
    expect(displayFrame.className).not.toMatch(/(^|\s)bg-white(\s|$)/)

    expect(screen.getByText('App')).toBeDefined()
    expect(screen.queryByText('Application')).toBeNull()
    const aboutFrame = screen.getByText('App').closest('div')!.parentElement!
    expect(aboutFrame).toHaveClass('border-y', 'bg-transparent')
    expect(aboutFrame.className).not.toContain('rounded-card')
    expect(aboutFrame.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(screen.getByRole('button', { name: 'Switch to dark' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Switch to Dark' })).toBeNull()
  })
})
