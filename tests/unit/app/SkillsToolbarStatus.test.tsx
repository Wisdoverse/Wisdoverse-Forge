import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { SkillsView } from '@app/features/skills/SkillsView'
import { useSkillsStore } from '@app/entities/skill'

const fetchMock = vi.fn()
vi.stubGlobal('fetch', fetchMock)

beforeEach(() => {
  fetchMock.mockReset()
  fetchMock.mockResolvedValue({
    ok: true,
    json: async () => ({ ok: true, data: [] }),
  })
  useSkillsStore.getState().reset()
})

afterEach(() => {
  cleanup()
  useSkillsStore.getState().reset()
})

describe('Skills toolbar status', () => {
  test('starts load errors with the recovery action', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 500,
      json: async () => ({ ok: false, error: 'API 500' }),
    })

    render(<SkillsView />)

    expect(await screen.findByText('Check saved guidance again to continue.')).toBeDefined()
    expect(screen.queryByText('Saved instructions need attention')).toBeNull()
  })

  test('keeps the empty catalog status visible for first-time users', async () => {
    render(<SkillsView />)

    await waitFor(() =>
      expect(screen.getByText('Choose Save guidance to start.')).toBeInTheDocument()
    )
    expect(screen.getByText(/Save steps your agents should repeat/i)).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions yet')).toBeNull()
    expect(screen.queryByText('Choose Save instruction or refresh this page.')).toBeNull()
  })

  test('explains when search hides every saved guidance item', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-release',
            name: 'release-review',
            description: 'Review release notes',
            trigger_pattern: 'release',
            content: 'Review release notes before publishing',
            enabled: true,
          },
        ],
      }),
    })

    render(<SkillsView />)

    await screen.findByText('release-review')
    fireEvent.change(screen.getByLabelText(/search saved guidance/i), {
      target: { value: 'database' },
    })

    expect(screen.getByText('Nothing matches your saved guidance search.')).toBeInTheDocument()
    expect(screen.getByText('Nothing matches your saved guidance search')).toBeInTheDocument()
    expect(
      screen.getByText('Use Show all saved guidance to return to the full list.')
    ).toBeInTheDocument()
    const emptyState = screen.getByTestId('saved-instructions-empty-state')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(
      within(emptyState).getByRole('button', { name: /show all saved guidance/i })
    ).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions match search')).toBeNull()
    expect(screen.queryByText('No saved instructions match this view')).toBeNull()

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all saved guidance/i }))

    expect(screen.getByText('release-review')).toBeInTheDocument()
    expect(screen.getByRole('searchbox', { name: /search saved guidance/i })).toHaveValue('')
  })

  test('explains when a filter hides every saved guidance item', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-release',
            name: 'release-review',
            description: 'Review release notes',
            trigger_pattern: 'release',
            content: 'Review release notes before publishing',
            enabled: true,
          },
        ],
      }),
    })

    render(<SkillsView />)

    await screen.findByText('release-review')
    fireEvent.click(
      screen.getByRole('button', {
        name: /show saved guidance for one work tool, 0 matching saved guidance items/i,
      })
    )

    expect(screen.getByText('Nothing matches this saved guidance view.')).toBeInTheDocument()
    expect(screen.getByText('Nothing matches this saved guidance view')).toBeInTheDocument()
    expect(
      screen.getByText('Use Show all saved guidance to return to the full list.')
    ).toBeInTheDocument()
    const emptyState = screen.getByTestId('saved-instructions-empty-state')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(
      within(emptyState).getByRole('button', { name: /show all saved guidance/i })
    ).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions match filter')).toBeNull()
    expect(screen.queryByText('Filter is hiding saved instructions')).toBeNull()
    expect(screen.queryByText('No saved instructions match this view')).toBeNull()

    fireEvent.click(within(emptyState).getByRole('button', { name: /show all saved guidance/i }))

    expect(screen.getByText('release-review')).toBeInTheDocument()
    expect(
      screen.getByRole('button', {
        name: /show all saved guidance, 1 matching saved guidance item/i,
      })
    ).toHaveAttribute('aria-pressed', 'true')
  })
})
