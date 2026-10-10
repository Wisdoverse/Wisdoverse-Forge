import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ErrorBoundary } from '@app/shared/ui/ErrorBoundary'
import { ErrorFallback } from '@app/shared/ui/ErrorFallback'
import { RouteErrorFallback } from '@app/shared/ui/RouteErrorFallback'
import { isChunkLoadError, recoverFromChunkError } from '@app/shared/lib/chunkError'
import * as chunkRecovery from '@app/shared/lib/chunkError'
import * as reliability from '@app/shared/lib/frontendReliability'

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

  test.each(['root', 'route'])('records %s failure before chunk recovery', (kind) => {
    const order: string[] = []
    vi.spyOn(reliability, 'recordFrontendCrash').mockImplementation(() => {
      order.push('record')
    })
    vi.spyOn(chunkRecovery, 'recoverFromChunkError').mockImplementation(() => {
      order.push('recover')
      return true
    })
    const error = new Error('Failed to fetch dynamically imported module')
    if (kind === 'root')
      render(
        <ErrorBoundary>
          <Boom error={error} />
        </ErrorBoundary>
      )
    else render(<RouteErrorFallback error={error} reset={vi.fn()} />)
    expect(order).toEqual(['record', 'recover'])
    expect(screen.getByRole('alert')).toHaveTextContent('Reloading can discard unsaved changes.')
  })

  test('renders the recovery UI on a non-chunk render throw — no blank screen', () => {
    render(
      <ErrorBoundary>
        <Boom error={new Error('kaboom')} />
      </ErrorBoundary>
    )
    expect(screen.getByTestId('error-fallback')).toBeInTheDocument()
    expect(screen.getByTestId('error-fallback-reload')).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('Reloading can discard unsaved changes.')
    expect(screen.getByRole('alert')).not.toHaveTextContent('server is unaffected')
    expect(screen.getByRole('alert')).not.toHaveTextContent('kaboom')
    expect(screen.getByRole('link', { name: 'Go to Tasks' })).toHaveAttribute('href', '/tasks')
  })

  test('shows the same recovery guidance after a route error', () => {
    render(<RouteErrorFallback error={new Error('internal-diagnostic')} reset={vi.fn()} />)

    expect(screen.getByRole('alert')).toHaveTextContent('Reloading can discard unsaved changes.')
    expect(screen.getByRole('alert')).not.toHaveTextContent('server is unaffected')
    expect(screen.getByRole('alert')).not.toHaveTextContent('internal-diagnostic')
    expect(screen.getByRole('button', { name: 'Reload' })).toBeEnabled()
    expect(screen.getByRole('link', { name: 'Go to Tasks' })).toHaveAttribute('href', '/tasks')
  })

  test('reloads when the user chooses recovery after the unsaved-change warning', async () => {
    const onReload = vi.fn()
    render(<ErrorFallback onReload={onReload} />)

    expect(screen.getByRole('alert')).toHaveTextContent('Reloading can discard unsaved changes.')
    expect(onReload).not.toHaveBeenCalled()
    await userEvent.setup().click(screen.getByRole('button', { name: 'Reload' }))
    expect(onReload).toHaveBeenCalledOnce()
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
