import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { ResourceMembersModal } from '@app/features/manage-members'
import type {
  AddResourceMemberInput,
  ResourceMember,
  UpdateResourceMemberInput,
} from '@app/entities/member'
import type { OrgUser } from '@app/entities/user'

afterEach(cleanup)

function makeUser(overrides: Partial<OrgUser>): OrgUser {
  return {
    id: 'user-1',
    email: 'builder@example.com',
    username: 'builder',
    role: 'member',
    ...overrides,
  }
}

function makeMember(overrides: Partial<ResourceMember>): ResourceMember {
  return {
    userId: 'user-1',
    email: 'builder@example.com',
    username: 'builder',
    role: 'member',
    ...overrides,
  }
}

function renderMembersModal({
  members = [],
  users = [makeUser({})],
  loadMembersError,
  loadUsersError,
  addMemberError,
  updateMemberError,
  removeMemberError,
}: {
  members?: ResourceMember[]
  users?: OrgUser[]
  loadMembersError?: unknown
  loadUsersError?: unknown
  addMemberError?: unknown
  updateMemberError?: unknown
  removeMemberError?: unknown
} = {}) {
  const loadMembers = loadMembersError
    ? vi.fn<() => Promise<ResourceMember[]>>().mockRejectedValue(loadMembersError)
    : vi.fn<() => Promise<ResourceMember[]>>().mockResolvedValue(members)
  const loadUsers = loadUsersError
    ? vi.fn<() => Promise<OrgUser[]>>().mockRejectedValue(loadUsersError)
    : vi.fn<() => Promise<OrgUser[]>>().mockResolvedValue(users)
  const addMember = vi
    .fn<(input: AddResourceMemberInput) => Promise<ResourceMember>>()
    .mockImplementation(async (input) => {
      if (addMemberError) throw addMemberError
      const user = users.find((item) => item.id === input.userId) ?? makeUser({ id: input.userId })
      return makeMember({
        userId: user.id,
        username: user.username,
        email: user.email,
        role: input.role,
      })
    })
  const updateMember = vi
    .fn<(userId: string, input: UpdateResourceMemberInput) => Promise<ResourceMember>>()
    .mockImplementation(async (userId, input) => {
      if (updateMemberError) throw updateMemberError
      const member = members.find((item) => item.userId === userId) ?? makeMember({ userId })
      return { ...member, role: input.role }
    })
  const removeMember = vi.fn<() => Promise<void>>().mockImplementation(async () => {
    if (removeMemberError) throw removeMemberError
  })
  const onClose = vi.fn()

  render(
    <ResourceMembersModal
      resourceLabel="Project"
      resourceName="Launch App"
      loadMembers={loadMembers}
      loadUsers={loadUsers}
      addMember={addMember}
      updateMember={updateMember}
      removeMember={removeMember}
      onClose={onClose}
    />
  )

  return { addMember, loadMembers, loadUsers, onClose, removeMember, updateMember }
}

describe('ResourceMembersModal', () => {
  test('guides users before adding the first resource member', async () => {
    const { addMember } = renderMembersModal()

    const guide = await screen.findByTestId('member-role-guide')
    expect(within(guide).getByText('Add people only when they need this project')).toBeDefined()
    expect(within(guide).getByText('Start with Member')).toBeDefined()
    expect(within(guide).getByText('Use Maintainer for daily setup')).toBeDefined()
    expect(within(guide).getByText('Reserve Owner and Admin')).toBeDefined()
    expect(
      screen.getByText(
        'Choose an organization user, pick the safest role, then add them to this resource.'
      )
    ).toBeDefined()

    const emptyState = screen.getByTestId('members-empty-state')
    expect(within(emptyState).getByText('No direct members yet')).toBeDefined()
    expect(
      within(emptyState).getByText(/Start with Member unless they need to manage access/i)
    ).toBeDefined()

    fireEvent.change(screen.getByLabelText('Select member to add'), {
      target: { value: 'user-1' },
    })
    fireEvent.click(screen.getByRole('button', { name: /add/i }))

    await waitFor(() => {
      expect(addMember).toHaveBeenCalledWith({ userId: 'user-1', role: 'member' })
    })
    expect(screen.getByText('builder')).toBeDefined()
  })

  test('explains that organization users must exist before members can be added', async () => {
    renderMembersModal({ users: [] })

    expect(await screen.findByText('No org users available')).toBeDefined()
    expect(
      screen.getByText('Invite a user to the organization first, then return here to grant access.')
    ).toBeDefined()
    expect(screen.getByLabelText('Select member to add')).toBeDisabled()
  })

  test('explains filtered candidate results without hiding current access', async () => {
    renderMembersModal({
      members: [makeMember({ userId: 'owner-1', username: 'owner', email: 'owner@example.com' })],
      users: [
        makeUser({ id: 'owner-1', username: 'owner', email: 'owner@example.com' }),
        makeUser({ id: 'reviewer-1', username: 'reviewer', email: 'reviewer@example.com' }),
      ],
    })

    expect(await screen.findByText('owner')).toBeDefined()
    fireEvent.change(screen.getByLabelText('Filter organization members'), {
      target: { value: 'missing-user' },
    })

    expect(screen.getByText('No matching org members')).toBeDefined()
    expect(
      screen.getByText(
        'Clear the filter or invite the person to the organization before adding them here.'
      )
    ).toBeDefined()
    expect(screen.getByText('owner@example.com')).toBeDefined()
  })

  test('shows beginner guidance when members cannot load', async () => {
    renderMembersModal({ loadMembersError: new Error('API 401: {"message":"token expired"}') })

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('Sign in again')
    expect(alert.textContent).not.toContain('Code:')
    expect(alert.textContent).not.toContain('API 401')
    expect(alert.textContent).not.toContain('token expired')
  })

  test('shows permission guidance when adding a member fails', async () => {
    renderMembersModal({ addMemberError: new Error('API 403: Forbidden') })

    await screen.findByText('No direct members yet')
    fireEvent.change(screen.getByLabelText('Select member to add'), {
      target: { value: 'user-1' },
    })
    fireEvent.click(screen.getByRole('button', { name: /add/i }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('You do not have permission')
    expect(alert.textContent).toContain('Ask an owner or admin')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('shows refresh guidance when role changes conflict', async () => {
    renderMembersModal({
      members: [makeMember({})],
      users: [makeUser({})],
      updateMemberError: new Error('API 409: {"message":"role already changed"}'),
    })

    await screen.findByText('builder')
    fireEvent.change(screen.getByLabelText('Role for builder'), {
      target: { value: 'admin' },
    })

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('This membership changed while you were editing')
    expect(alert.textContent).toContain('Refresh the members list')
    expect(alert.textContent).not.toContain('API 409')
    expect(alert.textContent).not.toContain('role already changed')
  })

  test('explains last-owner style remove failures without raw API text', async () => {
    renderMembersModal({
      members: [makeMember({ role: 'owner' })],
      users: [makeUser({})],
      removeMemberError: new Error('API 422: {"message":"Choose a different owner first."}'),
    })

    await screen.findByText('builder')
    fireEvent.click(screen.getByRole('button', { name: 'Remove builder' }))
    fireEvent.click(screen.getByRole('button', { name: 'Confirm remove builder' }))

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('Choose a different owner first')
    expect(alert.textContent).toContain('remove this person from this project')
    expect(alert.textContent).not.toContain('Details:')
    expect(alert.textContent).not.toContain('API 422')
  })
})
