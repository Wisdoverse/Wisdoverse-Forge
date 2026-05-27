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
}: {
  members?: ResourceMember[]
  users?: OrgUser[]
} = {}) {
  const loadMembers = vi.fn().mockResolvedValue(members)
  const loadUsers = vi.fn().mockResolvedValue(users)
  const addMember = vi
    .fn<(input: AddResourceMemberInput) => Promise<ResourceMember>>()
    .mockImplementation(async (input) => {
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
      const member = members.find((item) => item.userId === userId) ?? makeMember({ userId })
      return { ...member, role: input.role }
    })
  const removeMember = vi.fn<() => Promise<void>>().mockResolvedValue(undefined)
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
})
