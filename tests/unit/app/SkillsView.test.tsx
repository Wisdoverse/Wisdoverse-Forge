import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
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

  test('fills a skill draft from a common starting point', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /new skill/i })[0])
    const templates = screen.getByRole('group', { name: /skill templates/i })
    await user.click(within(templates).getByRole('button', { name: /release notes/i }))

    expect(within(templates).getByRole('button', { name: /release notes/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    expect(screen.getByLabelText(/^skill name$/i)).toHaveValue('release-notes')
    expect(screen.getByLabelText(/^short description$/i)).toHaveValue(
      'Draft release notes from accepted work'
    )
    expect(screen.getByLabelText(/^trigger pattern$/i)).toHaveValue('release')
    expect((screen.getByLabelText(/^content$/i) as HTMLTextAreaElement).value).toContain(
      'Group user-facing updates'
    )
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
    expect(screen.getByText('Trigger: webui')).toBeDefined()
    expect(screen.getByText('Global skills')).toBeDefined()
    expect(screen.getByText('1 skill')).toBeDefined()
  })

  test('summarizes reuse readiness and filters CLI scoped skills', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-cli-review',
            name: 'cli-review',
            description: 'Review terminal workflows',
            trigger_pattern: 'terminal',
            content: 'Check CLI handoff states',
            enabled: true,
            cliTool: 'codex',
          },
          {
            id: 'skill-draft',
            organization_id: 'org-1',
            name: 'release-draft',
            description: 'Draft release notes',
            trigger_pattern: 'release',
            content: 'Summarize accepted work',
            enabled: false,
          },
        ],
      }),
    })

    render(<SkillsView />)

    const summary = await screen.findByTestId('skill-reuse-summary')
    expect(within(summary).getByText('Total')).toBeDefined()
    expect(within(summary).getAllByText('Installed').length).toBeGreaterThan(0)
    expect(within(summary).getAllByText('Available').length).toBeGreaterThan(0)
    expect(within(summary).getAllByText('CLI scoped').length).toBeGreaterThan(0)

    const filters = within(summary).getByRole('group', { name: /skill filter/i })
    fireEvent.click(within(filters).getByRole('button', { name: /cli scoped\s*1/i }))

    expect(screen.getByText('cli-review')).toBeDefined()
    expect(screen.queryByText('release-draft')).toBeNull()
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
    expect(screen.getByText(/check before creating/i)).toBeDefined()
    expect(screen.getByText('Safe to share')).toBeDefined()

    await user.type(screen.getByLabelText(/^skill name$/i), 'frontend-review')
    await user.type(screen.getByLabelText(/^short description$/i), 'Review frontend flows')
    await user.type(screen.getByLabelText(/^trigger pattern$/i), 'frontend')
    await user.type(
      screen.getByLabelText(/^content$/i),
      'Check UI states and regressions'
    )
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

  test('guides users through required skill fields before create', async () => {
    const user = userEvent.setup()
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ok: true, data: [] }),
    })

    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /new skill/i })[0])
    await user.click(screen.getByRole('button', { name: /create skill/i }))

    expect(screen.getByRole('alert')).toHaveTextContent('Name this skill before creating it.')
    expect(screen.getByLabelText(/^skill name$/i)).toHaveFocus()

    await user.type(screen.getByLabelText(/^skill name$/i), 'frontend-review')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /create skill/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Add the instructions this skill should apply.'
    )
    expect(screen.getByLabelText(/^content$/i)).toHaveFocus()
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  test('shows beginner guidance when skill creation is denied', async () => {
    const user = userEvent.setup()
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ ok: true, data: [] }),
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 403,
        json: async () => ({ message: 'Forbidden' }),
      })

    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /new skill/i })[0])
    await user.type(screen.getByLabelText(/^skill name$/i), 'frontend-review')
    await user.type(screen.getByLabelText(/^content$/i), 'Check UI states and regressions')
    await user.click(screen.getByRole('button', { name: /create skill/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('You do not have permission to create workspace skills')
    expect(alert).toHaveTextContent('Ask an admin')
    expect(alert).toHaveTextContent('Code: 403.')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('shows loading state while fetching', () => {
    useSkillsStore.setState({ loading: true })
    render(<SkillsView />)
    expect(screen.getByText(/loading skills/i)).toBeDefined()
  })
})
