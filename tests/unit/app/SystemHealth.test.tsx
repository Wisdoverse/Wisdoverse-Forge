import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { SystemHealth } from '@app/features/admin/SystemHealth'
import { useAdminStore } from '@app/shared/model/admin.store'

const originalAdminState = useAdminStore.getState()

afterEach(() => {
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
          // docker intentionally absent → renders as "Not checked".
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
    expect(screen.getByText(/slow screens, delayed updates/i)).toBeDefined()
    expect(screen.getByText('Saved Data')).toBeDefined()
    expect(screen.getByText('Live Updates')).toBeDefined()
    expect(screen.getByText('Agent Containers')).toBeDefined()
    expect(screen.getByText('Docker runtime')).toBeDefined()
    expect(screen.getByText('12 ms response')).toBeDefined()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.getAllByText('Needs attention').length).toBeGreaterThan(0)
    expect(screen.getByText('Unavailable')).toBeDefined()
    expect(screen.getAllByText('Not checked').length).toBeGreaterThan(0)
    expect(screen.getByText(/API has been running for 2h/i)).toBeDefined()
    // The retired services from the old health endpoint are gone.
    expect(screen.queryByText('Agent Runner')).toBeNull()
    expect(screen.queryByText('Background Jobs')).toBeNull()
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

    expect(screen.getByText(/Could not load service readiness/i)).toBeDefined()
    expect(screen.getByText('HTTP 500')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDefined()
  })
})
