import { describe, test, expect, afterEach } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { ThemeProvider } from '@app/providers/ThemeProvider'
import { useTheme } from '@app/shared/model/theme.context'

afterEach(cleanup)

function ThemeDisplay() {
  const { theme, toggleTheme } = useTheme()
  return (
    <div>
      <span data-testid="theme">{theme}</span>
      <button onClick={toggleTheme}>Toggle</button>
    </div>
  )
}

describe('ThemeProvider', () => {
  test('defaults to light theme', () => {
    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )
    expect(screen.getByTestId('theme').textContent).toBe('light')
  })

  test('toggles between light and dark', async () => {
    const user = userEvent.setup()
    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )
    await user.click(screen.getByText('Toggle'))
    expect(screen.getByTestId('theme').textContent).toBe('dark')

    await user.click(screen.getByText('Toggle'))
    expect(screen.getByTestId('theme').textContent).toBe('light')
  })

  test('applies dark class to document element', async () => {
    const user = userEvent.setup()
    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )
    await user.click(screen.getByText('Toggle'))
    expect(document.documentElement.classList.contains('dark')).toBe(true)

    await user.click(screen.getByText('Toggle'))
    expect(document.documentElement.classList.contains('dark')).toBe(false)
  })
})
