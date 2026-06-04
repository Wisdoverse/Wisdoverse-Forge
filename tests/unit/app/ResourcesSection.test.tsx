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
    expect(
      within(guide).getByText('Pick the smallest profile that can finish the work')
    ).toBeDefined()
    expect(guide.textContent).toContain('Small is the smallest option')
    expect(within(guide).getByText('CPU controls speed')).toBeDefined()
    expect(within(guide).getByText('Memory prevents crashes')).toBeDefined()
    expect(within(guide).getByText('Limits protect the runner')).toBeDefined()

    expect(screen.getByText('Small')).toBeDefined()
    expect(screen.getByText(/1 core power · 1 GB memory/i)).toBeDefined()
    expect(screen.getByText('Light reviews, docs, and short commands')).toBeDefined()
    expect(screen.getByText('Normal coding tasks and test runs')).toBeDefined()
    expect(screen.getByText('Large builds, browser tests, and long-running work')).toBeDefined()
    expect(loadResourceProfilesMock).toHaveBeenCalled()
  })

  test('guides users when no resource profiles are configured', async () => {
    useSettingsStore.setState({ resourceProfiles: [] })

    render(<ResourcesSection />)

    const emptyState = await screen.findByTestId('resource-profiles-empty')
    expect(within(emptyState).getByText('No resource profiles are available yet')).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Agents need at least one profile before operators can choose CPU and memory limits.'
      )
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Ask an administrator to add profiles in platform configuration.'
      )
    ).toBeDefined()
    expect(
      within(emptyState).getByText(
        'Return here before creating container agents; at least one row means this step is ready.'
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
    expect(alert).toHaveTextContent('Agent sizes could not be loaded.')
    expect(alert).toHaveTextContent('Agent sizes decide how much CPU and memory')
    expect(alert).not.toHaveTextContent('HTTP 500')

    fireEvent.click(screen.getByRole('button', { name: /reload sizes/i }))
    expect(loadResourceProfilesMock).toHaveBeenCalledTimes(2)
  })
})
