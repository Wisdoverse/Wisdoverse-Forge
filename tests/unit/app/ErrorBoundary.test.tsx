import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ErrorBoundary } from '@app/shared/ui/ErrorBoundary'
import { isChunkLoadError, recoverFromChunkError } from '@app/shared/lib/chunkError'

function Boom({ error }: { error: Error }): never {
  throw error
}

describe('chunkError util (F069)', () => {
  test('detects chunk-load failures by name and message, ignores others', () => {
    expect(isChunkLoadError(Object.assign(new Error('x'), { name: 'ChunkLoadError' }))).toBe(true)
    expect(
      isChunkLoadError(new Error('Failed to fetch dynamically imported module: /assets/x.js'))
    ).toBe(true)
    expect(isChunkLoadError(new Error('Loading chunk 42 failed'))).toBe(true)
    expect(isChunkLoadError(new Error('some other error'))).toBe(false)
    expect(isChunkLoadError(null)).toBe(false)
  })
})

describe('ErrorBoundary (F069)', () => {
  beforeEach(() => {
    sessionStorage.clear()
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
  })

  test('renders children when there is no error', () => {
    render(
      <ErrorBoundary>
        <div data-testid="ok">fine</div>
      </ErrorBoundary>
    )
    expect(screen.getByTestId('ok')).toBeInTheDocument()
  })

  test('renders the recovery UI on a non-chunk render throw — no blank screen', () => {
    render(
      <ErrorBoundary>
        <Boom error={new Error('kaboom')} />
      </ErrorBoundary>
    )
    expect(screen.getByTestId('error-fallback')).toBeInTheDocument()
    expect(screen.getByTestId('error-fallback-reload')).toBeInTheDocument()
  })

  // The reload is injected (no window.location mocking — jsdom's location.reload
  // is non-configurable and cannot be spied).
  test('recoverFromChunkError reloads once then guards against a loop', () => {
    const reload = vi.fn()
    const chunkErr = Object.assign(new Error('Failed to fetch dynamically imported module'), {
      name: 'ChunkLoadError',
    })
    // First episode this session: reload triggered.
    expect(recoverFromChunkError(chunkErr, reload)).toBe(true)
    expect(reload).toHaveBeenCalledTimes(1)
    // Still broken in the same session: no second reload (loop guard).
    expect(recoverFromChunkError(chunkErr, reload)).toBe(false)
    expect(reload).toHaveBeenCalledTimes(1)
  })

  test('recoverFromChunkError ignores non-chunk errors', () => {
    const reload = vi.fn()
    expect(recoverFromChunkError(new Error('plain error'), reload)).toBe(false)
    expect(reload).not.toHaveBeenCalled()
  })
})
