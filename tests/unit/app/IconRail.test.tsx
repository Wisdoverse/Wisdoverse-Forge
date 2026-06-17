import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { IconRail } from '@app/layouts/IconRail'

afterEach(cleanup)

describe('IconRail', () => {
  test('uses readable navigation names for first-time users', () => {
    render(<IconRail activePath="/tasks" onNavigate={() => {}} />)

    expect(screen.getByRole('button', { name: 'Task board' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Updates inbox' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Managed agents' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Saved instructions' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Settings and setup' })).toBeDefined()
    expect(screen.getByRole('button', { name: 'Task board' })).toHaveAttribute(
      'aria-current',
      'page'
    )
  })

  test('navigates with the selected route path', async () => {
    const onNavigate = vi.fn()
    const user = userEvent.setup()

    render(<IconRail activePath="/tasks" onNavigate={onNavigate} />)

    await user.click(screen.getByRole('button', { name: 'Managed agents' }))

    expect(onNavigate).toHaveBeenCalledWith('/agents')
  })
})
