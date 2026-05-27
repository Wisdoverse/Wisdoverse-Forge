import { describe, test, expect, afterEach } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react'
import { CommandPalette } from '@app/features/cmdk/CommandPalette'

afterEach(cleanup)

describe('CommandPalette', () => {
  test('renders when open', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Find a page or action')).toBeDefined()
    expect(screen.getByText(/type what you want to do/i)).toBeDefined()
    expect(screen.getByPlaceholderText(/search pages and actions/i)).toBeDefined()
  })

  test('does not render when closed', () => {
    render(<CommandPalette isOpen={false} onClose={() => {}} />)
    expect(screen.queryByPlaceholderText(/search pages and actions/i)).toBeNull()
  })

  test('shows navigation commands', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Go to a page')).toBeDefined()
    expect(screen.getByText('Tasks')).toBeDefined()
    expect(screen.getByText('See work that is planned, active, or done.')).toBeDefined()
    expect(screen.getByText('Inbox')).toBeDefined()
    expect(screen.getByText('Agents')).toBeDefined()
  })

  test('shows action commands', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Start an action')).toBeDefined()
    expect(screen.getByText('Create task')).toBeDefined()
    expect(screen.getByText('Start a new piece of work.')).toBeDefined()
  })

  test('searches beginner descriptions and shows an empty state', async () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    const input = screen.getByPlaceholderText(/search pages and actions/i)
    fireEvent.change(input, { target: { value: 'alerts' } })

    await waitFor(() => {
      expect(screen.getByText('Inbox')).toBeDefined()
    })

    fireEvent.change(input, { target: { value: 'zzzzzz' } })

    await waitFor(() => {
      expect(screen.getByText('No matching page or action')).toBeDefined()
    })
    expect(screen.getByText(/try a simpler word/i)).toBeDefined()
  })
})
