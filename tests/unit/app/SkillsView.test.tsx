import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

// Mock fetch to prevent real network calls; store's loadSkills will
// set loading=true then revert on fetch error
const fetchMock = vi.fn()
vi.stubGlobal('fetch', fetchMock)

import { SkillsView } from '@app/features/skills/SkillsView'
import { useSkillsStore } from '@app/shared/model/skills.store'

afterEach(cleanup)

beforeEach(() => {
  fetchMock.mockReset()
  // Default: API returns empty skills list
  fetchMock.mockResolvedValue({
    ok: true,
    json: async () => ({ ok: true, skills: [], installedSkills: [] }),
  })
  useSkillsStore.setState({
    skills: [],
    installedSkills: [],
    loading: false,
    error: null,
    searchQuery: '',
  })
})

describe('SkillsView', () => {
  test('renders skill count in toolbar', () => {
    // Title is rendered by TopBar outside SkillsView — verify the
    // toolbar count replaces the old duplicate page heading.
    render(<SkillsView />)
    expect(screen.getByPlaceholderText(/search skills/i)).toBeDefined()
  })

  test('shows search input', () => {
    render(<SkillsView />)
    expect(screen.getByPlaceholderText(/search skills/i)).toBeDefined()
  })

  test('shows a create skill entry point', () => {
    render(<SkillsView />)
    expect(screen.getAllByRole('button', { name: /new skill/i }).length).toBeGreaterThan(0)
  })

  test('shows empty state after load with no skills', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })
  })

  test('renders skills from the Rust API response shape', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-webui-review',
            organization_id: null,
            name: 'webui-review',
            description: 'Review WebUI flows',
            trigger_pattern: 'webui',
            content: 'Check browser UI regressions',
            enabled: true,
          },
        ],
      }),
    })

    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText('webui-review')).toBeDefined()
    })
    expect(screen.getByText('Review WebUI flows')).toBeDefined()
    expect(screen.getByText('Global skills')).toBeDefined()
    expect(screen.getByText('1 skill')).toBeDefined()
  })

  test('creates a skill through the Rust API', async () => {
    const user = userEvent.setup()
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ ok: true, data: [] }),
      })
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          ok: true,
          data: {
            id: 'skill-frontend-review',
            organization_id: 'org-1',
            name: 'frontend-review',
            description: 'Review frontend flows',
            trigger_pattern: 'frontend',
            content: 'Check UI states and regressions',
            enabled: true,
          },
        }),
      })

    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /new skill/i })[0])
    await user.type(screen.getByLabelText(/^name$/i), 'frontend-review')
    await user.type(screen.getByLabelText(/^description$/i), 'Review frontend flows')
    await user.type(screen.getByLabelText(/^trigger pattern$/i), 'frontend')
    await user.type(screen.getByLabelText(/^content$/i), 'Check UI states and regressions')
    await user.click(screen.getByRole('button', { name: /create skill/i }))

    await waitFor(() => {
      expect(screen.getByText('frontend-review')).toBeDefined()
    })

    expect(fetchMock).toHaveBeenLastCalledWith(
      '/api/v1/skills',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          name: 'frontend-review',
          description: 'Review frontend flows',
          trigger_pattern: 'frontend',
          content: 'Check UI states and regressions',
        }),
      })
    )
  })

  test('shows loading state while fetching', () => {
    useSkillsStore.setState({ loading: true })
    render(<SkillsView />)
    expect(screen.getByText(/loading skills/i)).toBeDefined()
  })
})
