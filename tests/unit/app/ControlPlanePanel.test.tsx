import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { ControlPlanePanel } from '@app/features/admin/ControlPlanePanel'
import { controlPlaneErrorMessage } from '@app/features/admin/controlPlaneErrorMessage'
import { useAdminStore } from '@app/entities/admin'
import type { OrgControlPlaneSnapshot } from '@app/entities/admin'

const originalState = useAdminStore.getState()

const mockSnapshot: OrgControlPlaneSnapshot = {
  assignmentOutboxBacklog: 3,
  assignmentOutboxOldestAgeSeconds: 45,
  staleParticipants: 1,
  expiredWorkingLeases: 2,
  busyParticipantsWithoutWork: 0,
  workingTasksWithoutBusyParticipant: 4,
  staleAfterSeconds: 90,
}

beforeEach(() => {
  useAdminStore.setState({
    ...originalState,
    controlPlane: null,
    controlPlaneLoading: false,
    controlPlaneError: null,
    loadControlPlane: vi.fn().mockResolvedValue(undefined),
  })
})

afterEach(() => {
  cleanup()
  useAdminStore.setState(originalState, true)
  vi.restoreAllMocks()
})

describe('ControlPlanePanel', () => {
  test('renders the six signal labels and values from a mocked snapshot', () => {
    useAdminStore.setState({ controlPlane: mockSnapshot })

    render(<ControlPlanePanel />)

    // Six wedge-signal labels
    expect(screen.getByText('Work updates waiting to send')).toBeDefined()
    expect(screen.getByText('Oldest work update waiting (s)')).toBeDefined()
    expect(screen.getByText('Agents not checking in')).toBeDefined()
    expect(screen.getByText('Work check-ins overdue')).toBeDefined()
    expect(screen.getByText('Busy agents without work')).toBeDefined()
    expect(screen.getByText('Working tasks without a busy agent')).toBeDefined()

    // Values are rendered: non-zero warns, zero stays neutral, and both are visible.
    expect(screen.getByText('3')).toBeDefined()
    expect(screen.getByText('45s')).toBeDefined()
    expect(screen.getByText('1')).toBeDefined()
    expect(screen.getByText('2')).toBeDefined()
    expect(screen.getByText('0')).toBeDefined()
    expect(screen.getByText('4')).toBeDefined()
    expect(screen.getByLabelText('Work updates waiting to send: 3, check this value')).toBeDefined()
    expect(screen.queryByLabelText(/needs attention/i)).toBeNull()
    expect(screen.queryByText(/assignment updates/i)).toBeNull()

    expect(
      screen.getByText(/shows whether agents are getting work, checking in, and finishing tasks/i)
    ).toBeDefined()
    expect(screen.getByText(/forge checks this when admin opens, then every 30 seconds/i)).toBeDefined()
    expect(screen.getByText(/if any number below is above 0/i)).toBeDefined()
    expect(screen.queryByText(/this checks/i)).toBeNull()
    expect(screen.queryByText(/handoffs/i)).toBeNull()
    expect(screen.queryByText(/non-zero/i)).toBeNull()

    // Check-in timing context is explained in operator-facing language.
    expect(screen.getByText(/if an agent sends no update for 90s/i)).toBeDefined()
    expect(screen.queryByText(/check-in rule/i)).toBeNull()

    // Whole-app background work note is present without raw table names.
    expect(screen.getByText(/background tasks waiting across the platform/i)).toBeDefined()
    expect(screen.getByText(/check platform-wide numbers in their monitoring tools/i)).toBeDefined()
    expect(screen.queryByText(/\/metrics/)).toBeNull()
    expect(screen.queryByText(/background task backlog/i)).toBeNull()
    expect(screen.queryByText(/job_queue|platform-global|orchestration|wedged|lease/i)).toBeNull()
    expect(screen.getByText('Agent work checks')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Check again' })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Refresh' })).toBeNull()
    expect(screen.queryByText(/Control Plane/i)).toBeNull()
    expect(screen.queryByText(/Agent coordination/i)).toBeNull()
  })

  test('renders the error string when controlPlaneError is set', () => {
    useAdminStore.setState({
      controlPlane: null,
      controlPlaneLoading: false,
      controlPlaneError:
        'Open Admin and choose Agent work checks, then try again. Forge could not load agent work check status.',
    })

    render(<ControlPlanePanel />)

    const alert = screen.getByRole('alert')
    expect(alert).toBeDefined()
    expect(alert.textContent).toContain('Open Admin and choose Agent work checks')
    expect(alert.textContent).not.toContain('HTTP')
    expect(alert.textContent).not.toContain('stack')
    expect(alert.textContent).not.toContain('Control Plane')
    expect(alert.textContent).not.toContain('Agent coordination')
  })

  test('explains agent work checks loading as checking status', () => {
    useAdminStore.setState({ controlPlane: null, controlPlaneLoading: true })

    render(<ControlPlanePanel />)

    expect(screen.getByText('Checking agent work')).toBeDefined()
    expect(screen.queryByText('Loading Control Plane status')).toBeNull()
    expect(screen.queryByText('Checking agent coordination')).toBeNull()
  })

  test('a malformed or partial payload does not crash: zeros via numeric coercion', () => {
    // Simulate store already having applied num() coercion on a bad payload:
    // every field defaults to 0 when the raw value was missing/NaN.
    const zeroed: OrgControlPlaneSnapshot = {
      assignmentOutboxBacklog: 0,
      assignmentOutboxOldestAgeSeconds: 0,
      staleParticipants: 0,
      expiredWorkingLeases: 0,
      busyParticipantsWithoutWork: 0,
      workingTasksWithoutBusyParticipant: 0,
      staleAfterSeconds: 0,
    }
    useAdminStore.setState({ controlPlane: zeroed })

    // Should not throw
    expect(() => render(<ControlPlanePanel />)).not.toThrow()

    // All six rows render their labels
    expect(screen.getByText('Work updates waiting to send')).toBeDefined()
    expect(screen.getByText('Oldest work update waiting (s)')).toBeDefined()
    expect(screen.getByText('Agents not checking in')).toBeDefined()
    expect(screen.getByText('Work check-ins overdue')).toBeDefined()
    expect(screen.getByText('Busy agents without work')).toBeDefined()
    expect(screen.getByText('Working tasks without a busy agent')).toBeDefined()
  })

  test('maps technical access errors to a safe beginner recovery step', () => {
    const message = controlPlaneErrorMessage({ status: 403, message: 'owner role required' })

    expect(message).toBe(
      'Ask an owner or admin to give you Admin access, then open Admin and choose Agent work checks before choosing Check again. You do not have access to agent work check status.'
    )
    expect(message).not.toContain('role')
    expect(message).not.toContain('Control Plane')
    expect(message).not.toContain('Agent coordination')
  })

  test('maps plain role-required access errors to the same recovery step', () => {
    const message = controlPlaneErrorMessage('owner role required')

    expect(message).toBe(
      'Ask an owner or admin to give you Admin access, then open Admin and choose Agent work checks before choosing Check again. You do not have access to agent work check status.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('Agent coordination')
  })
})
