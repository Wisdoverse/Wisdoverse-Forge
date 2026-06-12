import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { AdminLayout } from '@app/features/admin/AdminLayout'
import { useAdminStore } from '@app/shared/model/admin.store'

vi.mock('@app/features/admin/UserManagement', () => ({
  UserManagement: () => <div>User access panel</div>,
}))

vi.mock('@app/features/admin/OrganizationsPanel', () => ({
  OrganizationsPanel: () => <div>Organizations panel</div>,
}))

vi.mock('@app/features/admin/SystemHealth', () => ({
  SystemHealth: () => <div>App health panel</div>,
}))

beforeEach(() => {
  useAdminStore.setState({ activeSection: 'users' })
})

afterEach(cleanup)

describe('AdminLayout', () => {
  test('uses plain-language admin navigation labels', () => {
    render(<AdminLayout />)

    expect(screen.getByText('Admin console')).toBeInTheDocument()
    expect(screen.getByLabelText('Admin area')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'User access' })).toHaveAttribute(
      'aria-current',
      'page'
    )
    expect(screen.getByRole('button', { name: 'Team spaces' })).toBeInTheDocument()
    expect(screen.getByText('App setup')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'App health' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Agent tool updates' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Organizations' })).toBeNull()
    expect(screen.queryByText(['System', 'status'].join(' '))).toBeNull()
    expect(screen.queryByRole('button', { name: ['Service', 'health'].join(' ') })).toBeNull()
    expect(screen.queryByRole('button', { name: ['App', 'readiness'].join(' ') })).toBeNull()
    expect(screen.queryByText(['Agent work', '-tool images'].join(''))).toBeNull()
  })

  test('switches to the selected admin area', () => {
    render(<AdminLayout />)

    fireEvent.click(screen.getByRole('button', { name: 'App health' }))

    expect(screen.getByText('App health panel')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'App health' })).toHaveAttribute(
      'aria-current',
      'page'
    )
  })
})
