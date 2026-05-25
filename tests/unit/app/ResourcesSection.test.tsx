import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, within } from '@testing-library/react'
import { ResourcesSection } from '@app/features/settings/ResourcesSection'
import { useSettingsStore } from '@app/shared/model/settings.store'

const loadResourceProfilesMock = vi.fn().mockResolvedValue(undefined)
const originalLoadResourceProfiles = useSettingsStore.getState().loadResourceProfiles

beforeEach(() => {
  loadResourceProfilesMock.mockClear()
  useSettingsStore.setState({
    resourceProfiles: [
      { id: 'small', name: 'Small', cpu: 1, memoryMb: 1024 },
      { id: 'medium', name: 'Medium', cpu: 2, memoryMb: 4096 },
      { id: 'large', name: 'Large', cpu: 4, memoryMb: 8192 },
    ],
    resourceProfilesLoading: false,
    resourceProfilesError: null,
    loadResourceProfiles: loadResourceProfilesMock,
  })
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
  useSettingsStore.setState({
    resourceProfiles: [],
    resourceProfilesLoading: false,
    resourceProfilesError: null,
    loadResourceProfiles: originalLoadResourceProfiles,
  })
})

describe('ResourcesSection', () => {
  test('explains agent sizes in beginner terms while keeping limits visible', () => {
    render(<ResourcesSection />)

    expect(loadResourceProfilesMock).toHaveBeenCalled()
    expect(screen.getByText('Agent Sizes')).toBeDefined()
    expect(screen.getByText(/Choose how much computer power/i)).toBeDefined()
    expect(screen.getByText(/Pick the smallest size that fits the job/i)).toBeDefined()

    const table = screen.getByRole('table')
    expect(within(table).getByText('Agent size')).toBeDefined()
    expect(within(table).getByText('Best for')).toBeDefined()
    expect(within(table).getByText('Computer limit')).toBeDefined()
    expect(within(table).getByText('Small')).toBeDefined()
    expect(within(table).getByText(/short chats, planning, and small file edits/i)).toBeDefined()
    expect(within(table).getByText(/2 cores power · 4 GB memory/i)).toBeDefined()
    expect(within(table).getByText(/larger builds, long searches/i)).toBeDefined()
  })

  test('shows a beginner-friendly empty state', () => {
    useSettingsStore.setState({ resourceProfiles: [] })

    render(<ResourcesSection />)

    expect(screen.getByText('No agent sizes available')).toBeDefined()
    expect(screen.getByText(/add at least one default size/i)).toBeDefined()
    expect(screen.getByText(/agents that work with files/i)).toBeDefined()
  })

  test('shows a recoverable loading error without exposing raw platform language', () => {
    useSettingsStore.setState({
      resourceProfiles: [],
      resourceProfilesError: 'backend failure',
    })

    render(<ResourcesSection />)

    expect(screen.getByText(/Could not load agent sizes/i)).toBeDefined()
    expect(screen.getByText(/Try refreshing this page/i)).toBeDefined()
    expect(screen.queryByText(/backend failure/i)).toBeNull()
  })
})
