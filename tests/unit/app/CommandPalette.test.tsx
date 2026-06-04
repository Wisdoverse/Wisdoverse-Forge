import { describe, test, expect, afterEach } from 'vitest'
import { fireEvent, render, screen, cleanup, waitFor } from '@testing-library/react'
import { CommandPalette } from '@app/features/cmdk/CommandPalette'

afterEach(cleanup)

describe('CommandPalette', () => {
  test('renders when open', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByPlaceholderText(/search/i)).toBeDefined()
    expect(screen.getByText('Command discovery path')).toBeDefined()
    expect(screen.getByText(/use tasks when you want to plan or inspect work/i)).toBeDefined()
    expect(screen.getByText(/use settings when setup/i)).toBeDefined()
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

    const input = screen.getByPlaceholderText(/search commands/i)
    fireEvent.change(input, { target: { value: 'alerts' } })

    await waitFor(() => {
      expect(screen.getByText('Inbox')).toBeDefined()
    })

    fireEvent.change(input, { target: { value: 'zzzzzz' } })

    await waitFor(() => {
      expect(screen.getByText('No command matches that search')).toBeDefined()
    })
    expect(screen.getByText(/try tasks, inbox, agents, skills, or settings/i)).toBeDefined()
    expect(screen.getByText(/clear the search if you are not sure what to type/i)).toBeDefined()
  })

  test('suggests common workflow terms when search has no matches', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText(/search commands/i), {
      target: { value: 'missing workflow' },
    })

    expect(screen.getByText('No command matches that search')).toBeDefined()
    expect(screen.getByText(/try tasks, inbox, agents, skills, or settings/i)).toBeDefined()
    expect(screen.getByText(/the full command list will come back/i)).toBeDefined()
  })
})
