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
  SystemHealth: () => <div>Service health panel</div>,
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
    expect(screen.getByRole('button', { name: 'Service health' })).toBeInTheDocument()
  })

  test('switches to the selected admin area', () => {
    render(<AdminLayout />)

    fireEvent.click(screen.getByRole('button', { name: 'Service health' }))

    expect(screen.getByText('Service health panel')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Service health' })).toHaveAttribute(
      'aria-current',
      'page'
    )
  })
})
