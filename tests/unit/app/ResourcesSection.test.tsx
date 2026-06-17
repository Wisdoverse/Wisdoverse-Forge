import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { ResourcesSection } from '@app/features/settings/ResourcesSection'
import type { ResourceProfileOption } from '@app/entities/agent'
import { useSettingsStore } from '@app/shared/model/settings.store'

const loadResourceProfilesMock = vi.fn().mockResolvedValue(undefined)
const originalLoadResourceProfiles = useSettingsStore.getState().loadResourceProfiles

const profiles: ResourceProfileOption[] = [
  { id: 'small', name: 'Small', cpu: 1, memoryMb: 1024 },
  { id: 'standard', name: 'Standard', cpu: 2, memoryMb: 4096 },
  { id: 'large', name: 'Large', cpu: 4, memoryMb: 8192 },
]

beforeEach(() => {
  loadResourceProfilesMock.mockClear()
  useSettingsStore.setState({
    resourceProfiles: profiles,
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
  test('explains resource profiles before users choose one for an agent', async () => {
    render(<ResourcesSection />)

    const guide = await screen.findByTestId('resource-profile-guide')
    expect(within(guide).getByText('Pick the smallest size that can finish the work')).toBeDefined()
    expect(guide.textContent).toContain('Small is the smallest size')
    expect(within(guide).getByText('Before choosing a size')).toBeDefined()
    expect(within(guide).getByText('More processing power speeds work up')).toBeDefined()
    expect(within(guide).getByText('More memory keeps large work stable')).toBeDefined()
    expect(within(guide).getByText('Sizes keep work fair for everyone')).toBeDefined()
    expect(
      within(guide).getByText('They keep one agent from using all shared work capacity.')
    ).toBeDefined()
    expect(within(guide).queryByText(/profile/i)).toBeNull()
    expect(within(guide).queryByText(/runner/i)).toBeNull()
    expect(within(guide).queryByText(/machine resources/i)).toBeNull()

    expect(screen.getByRole('columnheader', { name: 'Size' })).toBeDefined()
    expect(screen.getByRole('columnheader', { name: 'Good fit' })).toBeDefined()
    expect(screen.getByRole('columnheader', { name: 'Power and memory' })).toBeDefined()
    expect(screen.queryByRole('columnheader', { name: 'Profile' })).toBeNull()
    expect(screen.queryByRole('columnheader', { name: 'CPU' })).toBeNull()
    expect(screen.getByText('Small')).toBeDefined()
    expect(screen.getByText(/1 processing core · 1 GB memory/i)).toBeDefined()
    expect(screen.getByText('Light reviews, docs, and short commands')).toBeDefined()
    expect(screen.getByText('Normal coding tasks and test runs')).toBeDefined()
    expect(screen.getByText('Large builds, browser tests, and long-running work')).toBeDefined()
    expect(loadResourceProfilesMock).toHaveBeenCalled()
  })

  test('guides users when no resource profiles are configured', async () => {
    useSettingsStore.setState({ resourceProfiles: [] })

    render(<ResourcesSection />)

    const emptyState = await screen.findByTestId('resource-profiles-empty')
    expect(within(emptyState).getByText('Ask an owner or admin to add agent sizes')).toBeDefined()
    expect(within(emptyState).queryByText('No agent sizes are available yet')).toBeNull()
    expect(
      within(emptyState).getByText(
        'Agents need at least one size before users can choose safe work capacity.'
      )
    ).toBeDefined()
    expect(
      within(emptyState).getByText('Ask an owner or admin to add agent sizes in Work limits.')
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Return here before creating agents that edit project files; at least one row means this step is ready.'
      )
    ).toBeDefined()
  })

  test('shows a beginner retry path when agent sizes cannot load', async () => {
    useSettingsStore.setState({
      resourceProfilesError: 'HTTP 500',
      resourceProfilesLoading: false,
    })

    render(<ResourcesSection />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Reload sizes to load agent sizes.')
    expect(alert).toHaveTextContent('Agent sizes decide how much computer power and memory')
    expect(alert).toHaveTextContent('agent that edits project files')
    expect(alert).not.toHaveTextContent('managed workspace')
    expect(alert).not.toHaveTextContent('HTTP 500')

    fireEvent.click(screen.getByRole('button', { name: /reload sizes/i }))
    expect(loadResourceProfilesMock).toHaveBeenCalledTimes(2)
  })
})
