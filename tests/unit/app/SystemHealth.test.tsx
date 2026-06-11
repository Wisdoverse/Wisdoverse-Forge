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
  test('explains service readiness with user-facing labels', async () => {
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
          // docker intentionally absent so it renders as "Not checked".
        },
      },
      healthLoading: false,
      healthError: null,
      loadHealth,
    })

    render(<SystemHealth />)

    await waitFor(() => expect(loadHealth).toHaveBeenCalledOnce())
    expect(screen.getByText('Service readiness')).toBeDefined()
    expect(screen.getByText('Some services need attention')).toBeDefined()
    expect(
      screen.getByText(/slow screens, delayed updates, or work waiting to start/i)
    ).toBeDefined()
    expect(screen.getByText('Saved Data')).toBeDefined()
    expect(screen.getByText('App records service')).toBeDefined()
    expect(screen.getByText('Fast response helper')).toBeDefined()
    expect(screen.getByText('Progress update delivery')).toBeDefined()
    expect(screen.getByText('Agent Work Starter')).toBeDefined()
    expect(screen.getByText('Agent container service')).toBeDefined()
    expect(
      screen.getByText(/agent container service before sending new agent file work/i)
    ).toBeDefined()
    expect(screen.getByText('12 ms response')).toBeDefined()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.getAllByText('Needs attention').length).toBeGreaterThan(0)
    expect(screen.getByText('Unavailable')).toBeDefined()
    expect(screen.getAllByText('Not checked').length).toBeGreaterThan(0)
    expect(screen.getByText(/Service has been running for 2h/i)).toBeDefined()
    expect(screen.queryByText(/Background Jobs/i)).toBeNull()
    expect(screen.queryByText(/PostgreSQL/i)).toBeNull()
    expect(screen.queryByText(/Redis/i)).toBeNull()
    expect(screen.queryByText(/NATS/i)).toBeNull()
    expect(screen.queryByText(/Docker runtime/i)).toBeNull()
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

    expect(screen.getByText('All services are ready')).toBeDefined()
    expect(screen.getAllByText('Ready').length).toBe(4)
    expect(screen.queryByText('Not checked')).toBeNull()
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
    expect(screen.getByText(/every 30 seconds while this page is visible/i)).toBeDefined()

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

  test('hides raw service error details from readiness rows', async () => {
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
    expect(screen.getByText(/Support note:/i)).toBeDefined()
    expect(screen.getByText(/The service reported a connection problem/i)).toBeDefined()
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
    expect(screen.getByText(/Ask an owner or admin to check service setup/i)).toBeDefined()
    expect(screen.queryByText(/runtime configuration/i)).toBeNull()
  })

  test('uses clear loading copy while readiness is being checked', () => {
    useAdminStore.setState({
      ...originalAdminState,
      health: null,
      healthLoading: true,
      healthError: null,
      loadHealth: vi.fn(),
    })

    render(<SystemHealth />)

    expect(screen.getByText('Checking service readiness...')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Checking...' })).toBeDisabled()
  })

  test('explains what to do when readiness cannot load', () => {
    useAdminStore.setState({
      ...originalAdminState,
      health: null,
      healthLoading: false,
      healthError: 'HTTP 500',
      loadHealth: vi.fn(),
    })

    render(<SystemHealth />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Forge could not check service readiness. Refresh Admin, then choose Check now. If it still fails, ask an owner or admin to check service readiness setup.'
    )
    expect(screen.queryByText('HTTP 500')).toBeNull()
    expect(screen.queryByText(/temporarily unavailable/i)).toBeNull()
    expect(screen.queryByText(/admin service/i)).toBeNull()
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDefined()
  })
})
