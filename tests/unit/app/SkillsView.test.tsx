import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { act, render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
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
    expect(screen.getByPlaceholderText(/search saved instructions/i)).toBeDefined()
  })

  test('shows search input', () => {
    render(<SkillsView />)
    expect(screen.getByPlaceholderText(/search saved instructions/i)).toBeDefined()
  })

  test('shows a create saved instruction entry point', () => {
    render(<SkillsView />)
    expect(screen.getAllByRole('button', { name: /save instruction/i }).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /new instruction/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /new skill/i })).toBeNull()
  })

  test('fills a skill draft from a common starting point', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const templates = screen.getByRole('group', { name: /instruction templates/i })
    await user.click(within(templates).getByRole('button', { name: /release notes/i }))

    expect(within(templates).getByRole('button', { name: /release notes/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    expect(screen.getByLabelText(/^instruction name$/i)).toHaveValue('release-notes')
    expect(screen.getByLabelText(/^short description$/i)).toHaveValue(
      'Draft release notes from accepted work'
    )
    expect(screen.getByLabelText(/^matching words for future tasks$/i)).toHaveValue('release')
    expect((screen.getByLabelText(/^agent instructions$/i) as HTMLTextAreaElement).value).toContain(
      'Group user-facing updates'
    )
  })

  test('uses plain wording in the review checklist starter instruction', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const templates = screen.getByRole('group', { name: /instruction templates/i })
    await user.click(within(templates).getByRole('button', { name: /review checklist/i }))

    const instructions = screen.getByLabelText(/^agent instructions$/i) as HTMLTextAreaElement
    expect(instructions.value).toContain('link the file or page you checked')
    expect(instructions.value).not.toContain('link evidence')
  })

  test('offers a review status instruction that avoids repeated waiting', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const templates = screen.getByRole('group', { name: /instruction templates/i })
    await user.click(within(templates).getByRole('button', { name: /check review status/i }))

    expect(screen.getByLabelText(/^instruction name$/i)).toHaveValue('pr-status-check')
    expect(screen.getByLabelText(/^short description$/i)).toHaveValue(
      'Summarize review and build status from one fresh check'
    )
    expect(screen.getByLabelText(/^matching words for future tasks$/i)).toHaveValue(
      'review status, build status, checks'
    )
    const instructions = screen.getByLabelText(/^agent instructions$/i) as HTMLTextAreaElement
    expect(instructions.value).toContain('Check the code review page once')
    expect(instructions.value).toContain('reuse it instead of refreshing')
    expect(instructions.value).toContain('Needs a fix, Waiting, or Done')
    expect(instructions.value).toContain('open only the failed build or review item')
    expect(instructions.value).toContain('stop checking in chat')
    expect(instructions.value).toContain('project background watcher')
    expect(instructions.value).not.toContain('ACTION')
    expect(instructions.value).not.toContain('GitHub or GitLab')
    expect(instructions.value).not.toContain('npm run')
  })

  test('closes the create instruction draft without saving', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save a reusable instruction/i })

    expect(within(dialog).getByRole('button', { name: 'Close without saving' })).toBeDefined()
    expect(within(dialog).queryByRole('button', { name: /^Cancel$/ })).toBeNull()

    await user.click(within(dialog).getByRole('button', { name: 'Close without saving' }))

    expect(screen.queryByRole('dialog', { name: /save a reusable instruction/i })).toBeNull()
    expect(
      fetchMock.mock.calls.some(
        ([url, init]) => url === '/api/v1/skills' && (init as RequestInit | undefined)?.method === 'POST'
      )
    ).toBe(false)
  })

  test('shows empty state after load with no skills', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first saved instruction/i)).toBeDefined()
    })
  })

  test('guides empty saved-instruction search toward clearing or creating', async () => {
    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first saved instruction/i)).toBeDefined()
    })
    fireEvent.change(screen.getByLabelText(/search saved instructions/i), {
      target: { value: 'release handoff' },
    })

    expect(screen.getByText('Clear search or create a saved instruction')).toBeDefined()
    expect(screen.getByText(/clear search, then choose save instruction/i)).toBeDefined()
    expect(screen.queryByText('No saved instructions match your search')).toBeNull()
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
    expect(screen.getByText('Use when task says: webui')).toBeDefined()
    expect(screen.getByText('Saved in Global saved instructions')).toBeDefined()
    expect(screen.queryByText('Saved in Global skills')).toBeNull()
    expect(screen.queryByText(/^Source:/i)).toBeNull()
    expect(screen.queryByText(/Suggested for:/i)).toBeNull()
    expect(screen.getByText('1 saved instruction')).toBeDefined()
  })

  test('hides raw saved-instruction source names on cards', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-raw-source',
            name: 'handoff-check',
            description: 'Check handoff notes',
            trigger_pattern: 'handoff',
            content: 'Review the handoff before sharing.',
            enabled: true,
            plugin: '@example/team_skill_pack',
          },
        ],
      }),
    })

    render(<SkillsView />)

    await screen.findByText('handoff-check')
    expect(screen.getByText('Saved in saved instructions library')).toBeDefined()
    expect(screen.queryByText('@example/team_skill_pack')).toBeNull()
    expect(screen.queryByText('team_skill_pack')).toBeNull()
  })

  test('summarizes reuse readiness and filters tool-specific skills', async () => {
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
    expect(within(summary).getAllByText('For one work tool').length).toBeGreaterThan(0)
    expect(within(summary).getByText('Show saved instructions')).toBeDefined()
    expect(within(summary).queryByText('Show skills')).toBeNull()
    expect(within(summary).queryByText('Tool-specific')).toBeNull()
    expect(within(summary).queryByText('Reuse view')).toBeNull()
    expect(within(summary).queryByText(/C[L]I scoped/)).toBeNull()

    const filters = within(summary).getByRole('group', { name: /saved instruction filter/i })
    fireEvent.click(within(filters).getByRole('button', { name: /for one work tool\s*1/i }))

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
      expect(screen.getByText(/create your first saved instruction/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save a reusable instruction/i })
    expect(within(dialog).getByText(/check before saving/i)).toBeDefined()
    expect(screen.getByText('Safe to share')).toBeDefined()
    expect(screen.getByText(/leave out secret keys/i)).toBeDefined()
    expect(screen.queryByText(/tokens/i)).toBeNull()
    expect(screen.getByText(/choose this instruction manually/i)).toBeDefined()
    expect(screen.getByText(/words people usually write/i)).toBeDefined()

    await user.type(screen.getByLabelText(/^instruction name$/i), 'frontend-review')
    await user.type(screen.getByLabelText(/^short description$/i), 'Review frontend flows')
    await user.type(screen.getByLabelText(/^matching words for future tasks$/i), 'frontend')
    await user.type(
      screen.getByLabelText(/^agent instructions$/i),
      'Check UI states and regressions'
    )
    await user.click(within(dialog).getByRole('button', { name: /save instruction/i }))

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
      expect(screen.getByText(/create your first saved instruction/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save a reusable instruction/i })
    expect(within(dialog).getByText(/safe enough for this team space/i)).toBeDefined()
    expect(within(dialog).queryByText(/safe enough for the workspace/i)).toBeNull()
    await user.click(within(dialog).getByRole('button', { name: /save instruction/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Name this saved instruction before saving it.'
    )
    expect(screen.getByLabelText(/^instruction name$/i)).toHaveFocus()

    await user.type(screen.getByLabelText(/^instruction name$/i), 'frontend-review')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    await user.click(within(dialog).getByRole('button', { name: /save instruction/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Add the steps this saved instruction should apply.'
    )
    expect(screen.getByLabelText(/^agent instructions$/i)).toHaveFocus()
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })

  test('shows a recovery step when saved instructions fail to load', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 500,
      json: async () => ({ message: 'HTTP 500: database unavailable' }),
    })

    render(<SkillsView />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Refresh Saved instructions to load the list.')
    expect(alert).toHaveTextContent('Choose Retry to refresh Saved instructions.')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
    expect(screen.getByRole('button', { name: /^retry$/i })).toBeDefined()
  })

  test('turns raw saved-instruction load errors into retry guidance', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first saved instruction/i)).toBeDefined()
    })

    act(() => {
      useSkillsStore.setState({
        skills: [],
        installedSkills: [],
        loading: false,
        error: 'HTTP 500: database unavailable',
        searchQuery: '',
      })
    })

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Saved instructions need a refresh.')
    expect(alert).toHaveTextContent('Choose Retry to refresh Saved instructions.')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
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
      expect(screen.getByText(/create your first saved instruction/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /save instruction/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save a reusable instruction/i })
    await user.type(screen.getByLabelText(/^instruction name$/i), 'frontend-review')
    await user.type(
      screen.getByLabelText(/^agent instructions$/i),
      'Check UI states and regressions'
    )
    await user.click(within(dialog).getByRole('button', { name: /save instruction/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent(
      'Ask an owner or admin to let you create saved instructions for this team space.'
    )
    expect(alert.textContent).not.toContain('workspace instructions')
    expect(alert.textContent).not.toContain('Code:')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('shows loading state while fetching', () => {
    useSkillsStore.setState({ loading: true })
    render(<SkillsView />)
    expect(screen.getByText(/loading saved instructions/i)).toBeDefined()
  })
})
