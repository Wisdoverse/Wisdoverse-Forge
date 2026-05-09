import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { ThemeContext, type Theme } from '@app/shared/model/theme.context'

function getInitialTheme(): Theme {
  if (typeof window === 'undefined') return 'light'
  const stored = localStorage.getItem('agentforge-theme')
  if (stored === 'dark' || stored === 'light') return stored
  if (
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches
  )
    return 'dark'
  return 'light'
}

function applyThemeClass(t: Theme) {
  document.documentElement.classList.toggle('dark', t === 'dark')
  // Also set color-scheme so form controls, scrollbars, and the
  // native UA stylesheet render in the right palette.
  document.documentElement.style.colorScheme = t
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(getInitialTheme)

  // Apply the class/color-scheme on first mount AND whenever theme changes.
  // Previously the initial value from localStorage/prefers-color-scheme was
  // only set in state — the `dark` class was never put on the html element
  // until the user manually toggled, so dark mode silently failed on refresh.
  useEffect(() => {
    applyThemeClass(theme)
  }, [theme])

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t)
    localStorage.setItem('agentforge-theme', t)
    applyThemeClass(t)
  }, [])

  const toggleTheme = useCallback(() => {
    setTheme(theme === 'light' ? 'dark' : 'light')
  }, [theme, setTheme])

  return (
    <ThemeContext.Provider value={{ theme, toggleTheme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  )
}
