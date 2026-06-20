import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { SkillsView } from '@app/features/skills/SkillsView'
import { useSkillsStore } from '@app/shared/model/skills.store'

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

    expect(await screen.findByText('Load saved instructions again to continue.')).toBeDefined()
    expect(screen.queryByText('Saved instructions need attention')).toBeNull()
  })

  test('keeps the empty catalog status visible for first-time users', async () => {
    render(<SkillsView />)

    await waitFor(() =>
      expect(screen.getByText('Choose Save instruction to start.')).toBeInTheDocument()
    )
    expect(screen.getByText(/Save steps your agents should repeat/i)).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions yet')).toBeNull()
    expect(screen.queryByText('Choose Save instruction or refresh this page.')).toBeNull()
  })

  test('explains when search hides every saved instruction', async () => {
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
    fireEvent.change(screen.getByLabelText(/search saved instructions/i), {
      target: { value: 'database' },
    })

    expect(screen.getByText('Clear search to see saved instructions.')).toBeInTheDocument()
    expect(screen.getByText('Clear search to see saved instructions')).toBeInTheDocument()
    expect(screen.getByText(/this search hides them/i)).toBeInTheDocument()
    const emptyState = screen.getByTestId('saved-instructions-empty-state')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(
      within(emptyState).getByRole('button', { name: /show all saved instructions/i })
    ).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions match search')).toBeNull()
    expect(screen.queryByText('No saved instructions match this view')).toBeNull()

    fireEvent.click(
      within(emptyState).getByRole('button', { name: /show all saved instructions/i })
    )

    expect(screen.getByText('release-review')).toBeInTheDocument()
    expect(screen.getByRole('searchbox', { name: /search saved instructions/i })).toHaveValue('')
  })

  test('explains when a filter hides every saved instruction', async () => {
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
        name: /show saved instructions for one work tool, 0 matching saved instructions/i,
      })
    )

    expect(screen.getByText('Change filter to see saved instructions.')).toBeInTheDocument()
    expect(screen.getByText('Change filter to see saved instructions')).toBeInTheDocument()
    expect(screen.getByText(/this filter hides them/i)).toBeInTheDocument()
    const emptyState = screen.getByTestId('saved-instructions-empty-state')
    expect(emptyState).toHaveAttribute('role', 'status')
    expect(emptyState).toHaveAttribute('aria-live', 'polite')
    expect(
      within(emptyState).getByRole('button', { name: /show all saved instructions/i })
    ).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions match filter')).toBeNull()
    expect(screen.queryByText('No saved instructions match this view')).toBeNull()

    fireEvent.click(
      within(emptyState).getByRole('button', { name: /show all saved instructions/i })
    )

    expect(screen.getByText('release-review')).toBeInTheDocument()
    expect(
      screen.getByRole('button', {
        name: /show all saved instructions, 1 matching saved instruction/i,
      })
    ).toHaveAttribute('aria-pressed', 'true')
  })
})
