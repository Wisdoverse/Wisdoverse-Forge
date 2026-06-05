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
          platform: { status: 'degraded' },
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
    expect(screen.getByText('App records service')).toBeDefined()
    expect(screen.getByText('Fast response helper')).toBeDefined()
    expect(screen.getByText('Progress update delivery')).toBeDefined()
    expect(screen.getByText('Agent Work Starter')).toBeDefined()
    expect(screen.getByText('Agent work service')).toBeDefined()
    expect(screen.getByText(/agent work service before sending new agent work/i)).toBeDefined()
    expect(screen.queryByText(/PostgreSQL/i)).toBeNull()
    expect(screen.queryByText(/Redis/i)).toBeNull()
    expect(screen.queryByText(/NATS/i)).toBeNull()
    expect(screen.queryByText(/message bus/i)).toBeNull()
    expect(screen.queryByText(/runner/i)).toBeNull()
    expect(screen.queryByText(/container host/i)).toBeNull()
    expect(screen.queryByText(/container platform/i)).toBeNull()
    expect(screen.getByText('12 ms response')).toBeDefined()
    expect(screen.getByText('Ready')).toBeDefined()
    expect(screen.getAllByText('Needs attention').length).toBeGreaterThan(0)
    expect(screen.getByText('Unavailable')).toBeDefined()
    expect(screen.getAllByText('Not checked').length).toBeGreaterThan(0)
    expect(screen.getByText(/Service has been running for 2h/i)).toBeDefined()
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
      'Service readiness could not be loaded. Service readiness is temporarily unavailable. Ask an owner to check the admin service, then choose Check now.'
    )
    expect(screen.queryByText('HTTP 500')).toBeNull()
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDefined()
  })
})
