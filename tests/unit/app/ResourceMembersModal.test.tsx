import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import type { ComponentProps } from 'react'
import { ResourceMembersModal } from '@app/features/manage-members'
import type { ResourceMember } from '@app/entities/member'
import type { OrgUser } from '@app/entities/user'

const aliceMember: ResourceMember = {
  userId: 'user-alice',
  email: 'alice@example.com',
  username: 'Alice',
  role: 'member',
}

const bobUser: OrgUser = {
  id: 'user-bob',
  email: 'bob@example.com',
  username: 'Bob',
}

const defaultUsers: OrgUser[] = [
  {
    id: 'user-alice',
    email: 'alice@example.com',
    username: 'Alice',
  },
  bobUser,
]

function renderModal(overrides: Partial<ComponentProps<typeof ResourceMembersModal>> = {}) {
  const addMember = vi.fn(async () => ({
    userId: 'user-bob',
    email: 'bob@example.com',
    username: 'Bob',
    role: 'admin',
  }))
  const removeMember = vi.fn(async () => undefined)

  render(
    <ResourceMembersModal
      resourceLabel="Team"
      resourceName="Platform"
      loadMembers={async () => [aliceMember]}
      loadUsers={async () => defaultUsers}
      addMember={addMember}
      updateMember={vi.fn()}
      removeMember={removeMember}
      onClose={vi.fn()}
      {...overrides}
    />
  )

  return { addMember, removeMember }
}

afterEach(cleanup)

describe('ResourceMembersModal', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  test('guides a beginner through adding an existing organization member', async () => {
    const { addMember } = renderModal()

    expect(await screen.findByText('Add Existing Organization Members')).toBeInTheDocument()
    expect(screen.getByText(/search for a person, choose their role/i)).toBeInTheDocument()
    expect(screen.getByRole('status')).toHaveTextContent(/choose a person before adding them/i)
    expect(screen.getByText(/member: can view and work/i)).toBeInTheDocument()

    await userEvent.selectOptions(screen.getByLabelText(/select member to add/i), 'user-bob')
    await userEvent.selectOptions(screen.getByLabelText(/new member role/i), 'admin')

    expect(screen.getByRole('status')).toHaveTextContent(/ready to add bob as admin/i)
    expect(screen.getByText(/admin: can manage this resource/i)).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: /^add$/i }))

    await waitFor(() => {
      expect(addMember).toHaveBeenCalledWith({ userId: 'user-bob', role: 'admin' })
    })
  })

  test('confirms before removing a member', async () => {
    const { removeMember } = renderModal()

    await screen.findByText('Alice')
    await userEvent.click(screen.getByRole('button', { name: /remove alice/i }))

    expect(removeMember).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: /confirm remove alice/i })).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: /^cancel$/i }))
    expect(removeMember).not.toHaveBeenCalled()

    await userEvent.click(screen.getByRole('button', { name: /remove alice/i }))
    await userEvent.click(screen.getByRole('button', { name: /confirm remove alice/i }))

    await waitFor(() => {
      expect(removeMember).toHaveBeenCalledWith('user-alice')
    })
  })

  test('explains when everyone already has access', async () => {
    renderModal({ loadUsers: async () => [defaultUsers[0] as OrgUser] })

    await screen.findByText('Alice')
    expect(screen.getByRole('status')).toHaveTextContent(/everyone in the organization/i)
    expect(
      within(screen.getByRole('combobox', { name: /select member to add/i })).getByText(
        /no available org members/i
      )
    ).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^add$/i })).toBeDisabled()
  })
})
