import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { OrgSwitcher } from '@app/layouts/sidebar/OrgSwitcher'

const orgs = [
  { id: 'org-1', name: 'Design Studio', slug: 'design-studio', plan: 'pro', role: 'owner' },
  { id: 'org-2', name: 'Support Desk', slug: 'support-desk', plan: 'pro', role: 'member' },
]

afterEach(cleanup)

describe('OrgSwitcher', () => {
  it('explains team-space switching before selecting another team space', () => {
    const onSelect = vi.fn()
    render(<OrgSwitcher orgs={orgs} selectedOrgId="org-1" onSelect={onSelect} />)

    const switcher = screen.getByTestId('org-switcher')
    expect(switcher).toHaveAccessibleName('Team space selector: Design Studio')

    fireEvent.click(switcher)

    const dropdown = screen.getByRole('menu', { name: 'Choose team space' })
    expect(within(dropdown).getByText('Team space')).toBeInTheDocument()
    expect(
      within(dropdown).getByText('Switching changes which teams, projects, and Agents you can see.')
    ).toBeInTheDocument()

    fireEvent.click(within(dropdown).getByRole('menuitemradio', { name: 'Switch to Support Desk' }))

    expect(onSelect).toHaveBeenCalledWith('org-2')
    expect(screen.queryByTestId('org-dropdown')).not.toBeInTheDocument()
  })

  it('uses full team-space wording when nothing is selected', () => {
    render(<OrgSwitcher orgs={orgs} selectedOrgId={null} onSelect={vi.fn()} />)

    expect(screen.getByText('Select team space')).toBeInTheDocument()
    expect(screen.getByTestId('org-switcher')).toHaveAccessibleName(
      'Team space selector: Select team space'
    )
  })
})
