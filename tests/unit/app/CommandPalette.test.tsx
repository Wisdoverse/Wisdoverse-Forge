import { describe, test, expect, afterEach } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { CommandPalette } from '@app/features/cmdk/CommandPalette'

afterEach(cleanup)

describe('CommandPalette', () => {
  test('renders when open', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByPlaceholderText(/search/i)).toBeDefined()
  })

  test('does not render when closed', () => {
    render(<CommandPalette isOpen={false} onClose={() => {}} />)
    expect(screen.queryByPlaceholderText(/search/i)).toBeNull()
  })

  test('shows navigation commands', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Tasks')).toBeDefined()
    expect(screen.getByText('Inbox')).toBeDefined()
    expect(screen.getByText('Agents')).toBeDefined()
  })

  test('shows action commands', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Create Task')).toBeDefined()
  })
})
