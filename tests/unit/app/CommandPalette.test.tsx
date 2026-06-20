import { describe, test, expect, afterEach, vi } from 'vitest'
import { fireEvent, render, screen, cleanup, waitFor } from '@testing-library/react'
import { CommandPalette } from '@app/features/cmdk/CommandPalette'
import { useContextFeaturesStore } from '@app/shared/model/context-features.store'
import { useSettingsStore } from '@app/shared/model/settings.store'

afterEach(() => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useSettingsStore.setState({ preferences: null, preferencesLoaded: false })
})

describe('CommandPalette', () => {
  const previousDiscoveryTitle = ['Command', 'discovery', 'path'].join(' ')
  const previousEmptyTitle = ['No', 'command', 'matches', 'that', 'search'].join(' ')
  const previousFullListCopy = ['full', 'command', 'list'].join(' ')
  const previousActionHeading = ['Start', 'an', 'action'].join(' ')
  const previousSavedItemsLabel = new RegExp(`^${['Con', 'text'].join('')}$`)
  const previousSavedItemsDescription = new RegExp(
    ['Review', 'knowledge', 'before', 'agents', 'use', 'it', 'in', 'tasks'].join('\\s+'),
    'i'
  )

  test('renders when open', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    const dialog = screen.getByRole('dialog', { name: /find what you need/i })
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(dialog).toHaveAccessibleDescription(/use tasks when you want to plan or inspect work/i)
    expect(screen.getByLabelText('Search pages and things to do')).toBeDefined()
    expect(screen.getByPlaceholderText(/search/i)).toBeDefined()
    expect(screen.getByText('Find what you need')).toBeDefined()
    expect(screen.getByText(/use tasks when you want to plan or inspect work/i)).toBeDefined()
    expect(screen.getByText(/use inbox to check updates that need a next step/i)).toBeDefined()
    expect(screen.getByText(/use settings when setup, account access/i)).toBeDefined()
    expect(screen.queryByText(/something needs your attention/i)).toBeNull()
    expect(screen.queryByText(/runtime status/i)).toBeNull()
    expect(screen.queryByText(previousDiscoveryTitle)).toBeNull()
  })

  test('does not render when closed', () => {
    render(<CommandPalette isOpen={false} onClose={() => {}} />)
    expect(screen.queryByPlaceholderText(/search pages or things to do/i)).toBeNull()
  })

  test('shows navigation commands', () => {
    useContextFeaturesStore.setState({ governance: true, loaded: true, loading: false })

    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Go to a page')).toBeDefined()
    expect(screen.getByText('Tasks')).toBeDefined()
    expect(screen.getByText('See work that is planned, active, or done.')).toBeDefined()
    expect(screen.getByText('Inbox')).toBeDefined()
    expect(screen.getByText('Saved items')).toBeDefined()
    expect(
      screen.getByText('Check saved notes and instructions before agents reuse them.')
    ).toBeDefined()
    expect(screen.queryByText('Setup checklist')).toBeNull()
    expect(screen.getByText('Agents')).toBeDefined()
    expect(screen.getByText('Create or check agents that handle work.')).toBeDefined()
    expect(screen.getByText('Saved instructions')).toBeDefined()
    expect(screen.getByText('Reuse instructions for repeated work.')).toBeDefined()
    expect(screen.getByText('Connect tools, account access, teams, and projects.')).toBeDefined()
    expect(screen.queryByText(/workers doing tasks/i)).toBeNull()
    expect(screen.queryByText(previousSavedItemsLabel)).toBeNull()
    expect(screen.queryByText(previousSavedItemsDescription)).toBeNull()
    expect(screen.queryByText(/^Skills$/)).toBeNull()
    expect(screen.queryByText(/tools, keys/i)).toBeNull()
  })

  test('shows the setup checklist command only after Start is restored', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    expect(screen.getByText('Setup checklist')).toBeDefined()
    expect(
      screen.getByText('Open setup steps again when you want a guided checklist.')
    ).toBeDefined()

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'setup checklist' },
    })

    await waitFor(() => expect(screen.getByText('Setup checklist')).toBeDefined())
    fireEvent.click(screen.getByText('Open setup steps again when you want a guided checklist.'))

    expect(onSelect).toHaveBeenCalledWith('nav:start')
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(screen.queryByText('Show setup checklist again')).toBeNull()
  })

  test('finds setup checklist recovery when Start is hidden', async () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    expect(screen.queryByText('Setup checklist')).toBeNull()

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'start tutorial' },
    })

    await waitFor(() => {
      expect(screen.getByText('Reset setup checklist')).toBeDefined()
    })
    expect(
      screen.getByText(
        'Add the setup checklist back to the left menu and open it. Projects, agents, and tasks stay unchanged.'
      )
    ).toBeDefined()
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
  })

  test('shows action commands', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Create or change something')).toBeDefined()
    expect(screen.getByText('Reset setup checklist')).toBeDefined()
    expect(screen.getByText('New task')).toBeDefined()
    expect(screen.getByText('Create a task for an agent to finish.')).toBeDefined()
    expect(screen.getByText('Codex and work tool sign-in')).toBeDefined()
    expect(
      screen.getByText('Open the Codex sign-in page before agents work on project files.')
    ).toBeDefined()
    expect(screen.queryByText('Codex CLI sign-in')).toBeNull()
    expect(screen.queryByText(previousActionHeading)).toBeNull()
    expect(screen.queryByText('Create task')).toBeNull()
    expect(screen.queryByText('Start a new piece of work.')).toBeNull()
  })

  test('uses a beginner setup action when task creation is not ready', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()

    render(
      <CommandPalette
        isOpen={true}
        onClose={onClose}
        onSelect={onSelect}
        createTaskCommand={{
          label: 'Set up project before task',
          description: 'Open project settings so tasks have a place to belong.',
          searchText: 'new task create task project setup',
        }}
      />
    )

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'new task' },
    })

    await waitFor(() => {
      expect(screen.getByText('Set up project before task')).toBeDefined()
    })
    fireEvent.click(screen.getByText('Open project settings so tasks have a place to belong.'))

    expect(onSelect).toHaveBeenCalledWith('action:create-task')
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(screen.queryByText('Create a task for an agent to finish.')).toBeNull()
  })

  test('finds Codex sign-in through beginner search terms', async () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'codex login' },
    })

    await waitFor(() => {
      expect(screen.getByText('Codex and work tool sign-in')).toBeDefined()
    })
    expect(
      screen.getByText('Open the Codex sign-in page before agents work on project files.')
    ).toBeDefined()
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
  })

  test('uses beginner-safe view names instead of old scene jargon', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    expect(screen.getByText('Visual map')).toBeDefined()
    expect(screen.getByText('See agents and tasks on a visual map.')).toBeDefined()
    expect(screen.queryByText('3D view')).toBeNull()
    expect(screen.queryByText(/workshop/i)).toBeNull()
  })

  test('searches beginner descriptions and shows an empty state', async () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    const input = screen.getByPlaceholderText(/search pages or things to do/i)
    fireEvent.change(input, { target: { value: 'alerts' } })

    await waitFor(() => {
      expect(screen.getByText('Inbox')).toBeDefined()
    })

    fireEvent.change(input, { target: { value: 'zzzzzz' } })

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('No page or option matches that search')
    })
    expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite')
    expect(
      screen.getByText(/try tasks, inbox, agents, saved instructions, or settings/i)
    ).toBeDefined()
    expect(
      screen.queryByText(/try tasks, inbox, saved items, agents, saved instructions, or settings/i)
    ).toBeNull()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open settings/i })).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /show all pages and actions/i }))

    await waitFor(() => {
      expect(input).toHaveValue('')
      expect(screen.getByText('Tasks')).toBeDefined()
    })
    expect(screen.queryByRole('button', { name: 'Clear search' })).toBeNull()
    expect(screen.queryByText('No page or action matches that search')).toBeNull()
    expect(screen.queryByText(/common workflow/i)).toBeNull()
    expect(screen.queryByText(previousEmptyTitle)).toBeNull()
    expect(screen.queryByText(new RegExp(previousFullListCopy, 'i'))).toBeNull()
  })

  test('empty search lets beginners open a common page directly', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'where is the thing' },
    })

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('No page or option matches that search')
    })
    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onSelect).toHaveBeenCalledWith('nav:agents')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  test('suggests common workflow terms when search has no matches', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'missing workflow' },
    })

    expect(screen.getByText('No page or option matches that search')).toBeDefined()
    expect(
      screen.getByText(/try tasks, inbox, agents, saved instructions, or settings/i)
    ).toBeDefined()
    expect(screen.getByText(/open a page people use often/i)).toBeDefined()
    expect(
      screen.queryByText(/try tasks, inbox, saved items, agents, saved instructions, or settings/i)
    ).toBeNull()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open settings/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /show all pages and actions/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Clear search' })).toBeNull()
    expect(screen.queryByText(previousEmptyTitle)).toBeNull()
    expect(screen.queryByText(new RegExp(previousFullListCopy, 'i'))).toBeNull()
  })

  test('includes Saved items in empty-search help only when the page is visible', () => {
    useContextFeaturesStore.setState({ governance: true, loaded: true, loading: false })

    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText(/search pages or things to do/i), {
      target: { value: 'missing workflow' },
    })

    expect(
      screen.getByText(/try tasks, inbox, saved items, agents, saved instructions, or settings/i)
    ).toBeDefined()
  })
})
