import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  test('keeps the empty catalog status visible for first-time users', async () => {
    render(<SkillsView />)

    await waitFor(() =>
      expect(screen.getByText('Choose New Instruction to start.')).toBeInTheDocument()
    )
    expect(screen.getByText(/saved instructions are reusable steps/i)).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions yet')).toBeNull()
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
    expect(screen.getByText(/adjust search or filters/i)).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions match search')).toBeNull()
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
    fireEvent.click(screen.getByRole('button', { name: /for one work tool\s*0/i }))

    expect(screen.getByText('Change filter to see saved instructions.')).toBeInTheDocument()
    expect(screen.getByText(/adjust search or filters/i)).toBeInTheDocument()
    expect(screen.queryByText('No saved instructions match filter')).toBeNull()
  })
})
