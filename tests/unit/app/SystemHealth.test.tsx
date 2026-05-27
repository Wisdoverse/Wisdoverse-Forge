import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { SystemHealth } from '@app/features/admin/SystemHealth'
import {
  useAdminStore,
  type SystemHealth as SystemHealthState,
} from '@app/shared/model/admin.store'

const loadHealthMock = vi.fn().mockResolvedValue(undefined)
const originalLoadHealth = useAdminStore.getState().loadHealth

const healthyState: SystemHealthState = {
  status: 'healthy',
  checks: {
    database: { status: 'up', latencyMs: 8 },
    redis: { status: 'up', latencyMs: 4 },
    nats: { status: 'up', latencyMs: 6 },
    platform: { status: 'up', latencyMs: 12 },
    bullmq: { status: 'up', latencyMs: 9 },
  },
  uptime: 3661,
}

beforeEach(() => {
  loadHealthMock.mockClear()
  useAdminStore.setState({
    health: healthyState,
    healthLoading: false,
    healthError: null,
    loadHealth: loadHealthMock,
  })
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
  useAdminStore.setState({
    health: null,
    healthLoading: false,
    healthError: null,
    loadHealth: originalLoadHealth,
  })
})

describe('SystemHealth', () => {
  test('explains a healthy system in user-facing language', async () => {
    render(<SystemHealth />)

    await waitFor(() => expect(loadHealthMock).toHaveBeenCalledWith())
    expect(screen.getByText('Everything is working')).toBeDefined()
    expect(
      screen.getByText(
        'Users should be able to open the app, start agent work, and see updates normally.'
      )
    ).toBeDefined()
    expect(screen.getByText('Saved Data')).toBeDefined()
    expect(screen.getByText('PostgreSQL database')).toBeDefined()
    expect(screen.getAllByText('Working normally')).toHaveLength(5)
    expect(
      screen.getByText(
        'The system has been running for about 1 hour. This page checks again every 30 seconds.'
      )
    ).toBeDefined()

    fireEvent.click(screen.getByRole('button', { name: /check again/i }))
    expect(loadHealthMock).toHaveBeenCalledTimes(2)
  })

  test('shows impact and next steps when services need attention', () => {
    useAdminStore.setState({
      health: {
        status: 'degraded',
        checks: {
          database: { status: 'up', latencyMs: 10 },
          redis: { status: 'degraded', latencyMs: 120, error: 'Cache connection is slow' },
          nats: { status: 'down', error: 'Cannot connect to message broker' },
          bullmq: { status: 'up', latencyMs: 11 },
        },
      },
    })

    render(<SystemHealth />)

    expect(screen.getByText('Some parts need attention')).toBeDefined()
    expect(
      screen.getByText('3 areas may be slower or unreliable. Review the next steps below.')
    ).toBeDefined()
    expect(screen.getByText('Fast Loading')).toBeDefined()
    expect(screen.getByText('Needs attention')).toBeDefined()
    expect(
      screen.getByText(
        'User impact: The app can still work, but pages and realtime updates may feel slower.'
      )
    ).toBeDefined()
    expect(
      screen.getByText(
        'Next step: Wait a minute and refresh. If it stays degraded, restart the cache service.'
      )
    ).toBeDefined()
    expect(screen.getByText('Reported detail: Cache connection is slow')).toBeDefined()
    expect(screen.getByText('Live Updates')).toBeDefined()
    expect(screen.getByText('Not working')).toBeDefined()
    expect(screen.getByText('Agent Runner')).toBeDefined()
    expect(screen.getByText('Not checked yet')).toBeDefined()
  })

  test('guides the operator when health cannot be loaded yet', () => {
    useAdminStore.setState({
      health: null,
      healthLoading: false,
      healthError: 'HTTP 500',
    })

    render(<SystemHealth />)

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Health could not be loaded. Check that the API is running, then try again. Detail: HTTP 500'
    )
  })
})
