import { afterEach, describe, expect, test, vi } from 'vitest'
import { act, cleanup, render, screen, waitFor } from '@testing-library/react'
import { SystemHealth } from '@app/features/admin/SystemHealth'
import { useAdminStore } from '@app/shared/model/admin.store'

const originalAdminState = useAdminStore.getState()

afterEach(() => {
  vi.useRealTimers()
  cleanup()
  useAdminStore.setState(originalAdminState, true)
  vi.restoreAllMocks()
})

describe('SystemHealth', () => {
  test('explains app health with user-facing labels', async () => {
    const loadHealth = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      health: {
        status: 'degraded',
        uptime: 7200,
        checks: {
          database: { status: 'up', latencyMs: 12 },
          redis: { status: 'degraded' },
          nats: { status: 'down' },
          // docker intentionally absent so it renders as the check action.
        },
      },
      healthLoading: false,
      healthError: null,
      loadHealth,
    })

    render(<SystemHealth />)

    await waitFor(() => expect(loadHealth).toHaveBeenCalledOnce())
    expect(screen.getByText('App health check')).toBeDefined()
    expect(
      screen.getByText(/Start with anything marked Fix first, then items marked Check soon/i)
    ).toBeDefined()
    expect(screen.getByText('Some areas need a check')).toBeDefined()
    expect(
      screen.getByText(/slow screens, delayed updates, or work waiting to start/i)
    ).toBeDefined()
    expect(screen.getByText('Saved Data')).toBeDefined()
    expect(screen.getByText('Keeps saved work available')).toBeDefined()
    expect(screen.getByText('Helps pages load quickly')).toBeDefined()
    expect(screen.getByText('Shows new progress in the browser')).toBeDefined()
    expect(screen.getByText('Agent Work Starter')).toBeDefined()
    expect(screen.getByText('Starts file-work agents')).toBeDefined()
    expect(
      screen.getByText(/ask an owner or admin to check managed workspace setup/i)
    ).toBeDefined()
    expect(screen.getByText('responds in 12 ms')).toBeDefined()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.getAllByText('Check soon').length).toBeGreaterThan(0)
    expect(screen.getByText('Fix first')).toBeDefined()
    expect(screen.queryByText('Needs attention')).toBeNull()
    expect(screen.queryByText('Unavailable')).toBeNull()
    expect(screen.getByText('Choose Check now to confirm')).toBeDefined()
    expect(screen.getAllByText('Check now').length).toBeGreaterThan(1)
    expect(screen.getByText(/Forge has been running for 2h/i)).toBeDefined()
    expect(screen.queryByText(/Background Jobs/i)).toBeNull()
    expect(screen.queryByText(/PostgreSQL/i)).toBeNull()
    expect(screen.queryByText(/Redis/i)).toBeNull()
    expect(screen.queryByText(/NATS/i)).toBeNull()
    expect(screen.queryByText(/Docker runtime/i)).toBeNull()
    expect(screen.queryByText(/container service/i)).toBeNull()
  })

  test('shows every check as ready when the probe reports all dependencies up', () => {
    useAdminStore.setState({
      ...originalAdminState,
      health: {
        status: 'healthy',
        checks: {
          database: { status: 'up' },
          redis: { status: 'up' },
          nats: { status: 'up' },
          docker: { status: 'up' },
        },
      },
      healthLoading: false,
      healthError: null,
      loadHealth: vi.fn(),
    })

    render(<SystemHealth />)

    expect(screen.getByText('All areas are ready')).toBeDefined()
    expect(screen.getAllByText('Ready').length).toBe(4)
    expect(screen.queryByText('Not checked')).toBeNull()
    expect(screen.queryByText('Choose Check now to confirm')).toBeNull()
  })

  test('pauses automatic checks while the admin page is hidden', async () => {
    vi.useFakeTimers()
    const loadHealth = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      health: {
        status: 'healthy',
        checks: {
          database: { status: 'up' },
          redis: { status: 'up' },
          nats: { status: 'up' },
          docker: { status: 'up' },
        },
      },
      healthLoading: false,
      healthError: null,
      loadHealth,
    })

    render(<SystemHealth />)

    expect(loadHealth).toHaveBeenCalledOnce()
    expect(screen.getByText(/refreshes every 30 seconds while Admin is open/i)).toBeDefined()
    expect(screen.queryByText(/Hidden tabs pause checks/i)).toBeNull()
    expect(screen.queryByText(/while this page is visible/i)).toBeNull()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })
    expect(loadHealth).toHaveBeenCalledTimes(2)

    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })
    expect(loadHealth).toHaveBeenCalledTimes(2)
  })

  test('hides raw service error details from health check rows', async () => {
    const loadHealth = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      health: {
        status: 'unhealthy',
        checks: {
          database: {
            status: 'down',
            error: 'connection refused at postgres.internal:5432 stack trace line 7',
          },
        },
      },
      healthLoading: false,
      healthError: null,
      loadHealth,
    })

    render(<SystemHealth />)

    await waitFor(() => expect(loadHealth).toHaveBeenCalledOnce())
    expect(screen.getByText(/Owner\/admin note:/i)).toBeDefined()
    expect(screen.getByText(/This area reported a connection problem/i)).toBeDefined()
    expect(screen.queryByText(/postgres\.internal/i)).toBeNull()
    expect(screen.queryByText(/5432/i)).toBeNull()
    expect(screen.queryByText(/stack trace/i)).toBeNull()
    expect(screen.queryByText(/Reported detail/i)).toBeNull()
  })

  test('turns service setup errors into owner or admin next steps', async () => {
    const loadHealth = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      health: {
        status: 'unhealthy',
        checks: {
          docker: {
            status: 'down',
            error: 'missing runtime configuration value',
          },
        },
      },
      healthLoading: false,
      healthError: null,
      loadHealth,
    })

    render(<SystemHealth />)

    await waitFor(() => expect(loadHealth).toHaveBeenCalledOnce())
    expect(screen.getByText(/Ask an owner or admin to check app setup/i)).toBeDefined()
    expect(screen.queryByText(/runtime configuration/i)).toBeNull()
  })

  test('uses clear loading copy while app health is being checked', () => {
    useAdminStore.setState({
      ...originalAdminState,
      health: null,
      healthLoading: true,
      healthError: null,
      loadHealth: vi.fn(),
    })

    render(<SystemHealth />)

    expect(screen.getByText('Checking app health now')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Checking now' })).toBeDisabled()
    expect(screen.queryByText('Checking app health...')).toBeNull()
  })

  test('explains what to do when app health cannot load', () => {
    useAdminStore.setState({
      ...originalAdminState,
      health: null,
      healthLoading: false,
      healthError: 'HTTP 500',
      loadHealth: vi.fn(),
    })

    render(<SystemHealth />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Refresh Admin, then choose Check now. Forge could not check app health. If it still fails, ask an owner or admin to check app health setup.'
    )
    expect(screen.queryByText('HTTP 500')).toBeNull()
    expect(screen.queryByText(/temporarily unavailable/i)).toBeNull()
    expect(screen.queryByText(/admin service/i)).toBeNull()
    expect(screen.queryByText(/service readiness/i)).toBeNull()
    expect(screen.queryByText(new RegExp(['app', 'readiness'].join(' '), 'i'))).toBeNull()
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDefined()
  })
})
