import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest'
import { act, render, screen, cleanup } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { ThemeProvider } from '@app/providers/ThemeProvider'
import { useTheme } from '@app/shared/model/theme.context'

const STORAGE_KEY = 'agentforge-theme'

type MediaQueryListListener = (event: MediaQueryListEvent) => void

interface FakeMediaQueryList {
  matches: boolean
  media: string
  addEventListener: (type: 'change', listener: MediaQueryListListener) => void
  removeEventListener: (type: 'change', listener: MediaQueryListListener) => void
  dispatchEvent: (event: MediaQueryListEvent) => boolean
  addListener: (listener: MediaQueryListListener) => void
  removeListener: (listener: MediaQueryListListener) => void
  onchange: MediaQueryListListener | null
  trigger: (matches: boolean) => void
}

function createMatchMedia(initial: boolean) {
  const listeners = new Set<MediaQueryListListener>()
  const list: FakeMediaQueryList = {
    matches: initial,
    media: '(prefers-color-scheme: dark)',
    addEventListener: (_type, listener) => {
      listeners.add(listener)
    },
    removeEventListener: (_type, listener) => {
      listeners.delete(listener)
    },
    dispatchEvent: () => true,
    addListener: (listener) => listeners.add(listener),
    removeListener: (listener) => listeners.delete(listener),
    onchange: null,
    trigger: (matches) => {
      list.matches = matches
      const event = { matches, media: list.media } as MediaQueryListEvent
      listeners.forEach((l) => l(event))
    },
  }
  const matchMedia = vi.fn(() => list as unknown as MediaQueryList)
  return { matchMedia, list }
}

function ThemeDisplay() {
  const { theme, toggleTheme, setTheme } = useTheme()
  return (
    <div>
      <span data-testid="theme">{theme}</span>
      <button onClick={toggleTheme}>Toggle</button>
      <button onClick={() => setTheme('dark')}>SetDark</button>
      <button onClick={() => setTheme('light')}>SetLight</button>
    </div>
  )
}

const originalMatchMedia = window.matchMedia

beforeEach(() => {
  window.localStorage.clear()
  document.documentElement.removeAttribute('data-theme')
  document.documentElement.classList.remove('dark')
  document.documentElement.style.colorScheme = ''
  if (!document.querySelector('meta[name="theme-color"]')) {
    const meta = document.createElement('meta')
    meta.setAttribute('name', 'theme-color')
    meta.setAttribute('content', '#f5f5f7')
    document.head.appendChild(meta)
  }
})

afterEach(() => {
  cleanup()
  window.matchMedia = originalMatchMedia
})

describe('ThemeProvider', () => {
  test('defaults to light when no stored pref and system prefers light', () => {
    const { matchMedia } = createMatchMedia(false)
    window.matchMedia = matchMedia

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    expect(screen.getByTestId('theme').textContent).toBe('light')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(document.documentElement.style.colorScheme).toBe('light')
  })

  test('honors prefers-color-scheme: dark on first paint', () => {
    const { matchMedia } = createMatchMedia(true)
    window.matchMedia = matchMedia

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    expect(screen.getByTestId('theme').textContent).toBe('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(document.documentElement.classList.contains('dark')).toBe(true)
  })

  test('reads pre-paint data-theme attribute when present', () => {
    document.documentElement.setAttribute('data-theme', 'dark')
    const { matchMedia } = createMatchMedia(false)
    window.matchMedia = matchMedia

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    expect(screen.getByTestId('theme').textContent).toBe('dark')
  })

  test('reads stored theme over system preference', () => {
    window.localStorage.setItem(STORAGE_KEY, 'dark')
    const { matchMedia } = createMatchMedia(false)
    window.matchMedia = matchMedia

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    expect(screen.getByTestId('theme').textContent).toBe('dark')
  })

  test('toggle persists choice to localStorage and updates DOM', async () => {
    const { matchMedia } = createMatchMedia(false)
    window.matchMedia = matchMedia
    const user = userEvent.setup()

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    await user.click(screen.getByText('Toggle'))
    expect(screen.getByTestId('theme').textContent).toBe('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('dark')

    await user.click(screen.getByText('Toggle'))
    expect(screen.getByTestId('theme').textContent).toBe('light')
    expect(document.documentElement.getAttribute('data-theme')).toBe('light')
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('light')
  })

  test('updates meta theme-color on theme change', async () => {
    const { matchMedia } = createMatchMedia(false)
    window.matchMedia = matchMedia
    const user = userEvent.setup()

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    const meta = document.querySelector('meta[name="theme-color"]') as HTMLMetaElement
    expect(meta.getAttribute('content')).toBe('#f5f5f7')

    await user.click(screen.getByText('SetDark'))
    expect(meta.getAttribute('content')).toBe('#0f172a')

    await user.click(screen.getByText('SetLight'))
    expect(meta.getAttribute('content')).toBe('#f5f5f7')
  })

  test('follows system preference when no explicit choice stored', () => {
    const { matchMedia, list } = createMatchMedia(false)
    window.matchMedia = matchMedia

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    expect(screen.getByTestId('theme').textContent).toBe('light')

    act(() => {
      list.trigger(true)
    })

    expect(screen.getByTestId('theme').textContent).toBe('dark')
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  test('ignores system preference change once user has stored a choice', async () => {
    const { matchMedia, list } = createMatchMedia(false)
    window.matchMedia = matchMedia
    const user = userEvent.setup()

    render(
      <ThemeProvider>
        <ThemeDisplay />
      </ThemeProvider>
    )

    await user.click(screen.getByText('SetLight'))
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('light')

    act(() => {
      list.trigger(true)
    })

    expect(screen.getByTestId('theme').textContent).toBe('light')
  })
})
