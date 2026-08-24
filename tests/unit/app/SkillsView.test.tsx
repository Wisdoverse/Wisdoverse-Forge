import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest'
import { act, render, screen, cleanup, fireEvent, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

// Mock fetch to prevent real network calls; store's loadSkills will
// set loading=true then revert on fetch error
const fetchMock = vi.fn()
vi.stubGlobal('fetch', fetchMock)

// Analytics events are best-effort side-channel data; keep skill tests
// focused on the skills flow.
vi.mock('@app/shared/api/orchestration', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@app/shared/api/orchestration')>()
  return {
    ...actual,
    orchestrationApi: {
      ...actual.orchestrationApi,
      trackProductEvent: vi.fn().mockResolvedValue(undefined),
      listAnalyticsEvents: vi.fn().mockResolvedValue([]),
    },
  }
})

import { SkillsView } from '@app/features/skills/SkillsView'
import { useSkillsStore } from '@app/entities/skill'

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

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((res) => {
    resolve = res
  })
  return { promise, resolve }
}

describe('SkillsView', () => {
  test('renders skill count in toolbar', () => {
    // Title is rendered by TopBar outside SkillsView — verify the
    // toolbar count replaces the old duplicate page heading.
    render(<SkillsView />)
    const search = screen.getByRole('searchbox', { name: /search skills/i })
    expect(search).toHaveAccessibleDescription(
      'Search only narrows this list. Use Show all skills to return to the full list.'
    )
  })

  test('shows search input', () => {
    render(<SkillsView />)
    expect(screen.getByPlaceholderText(/search skills/i)).toBeDefined()
  })

  test('shows a create saved guidance entry point', () => {
    render(<SkillsView />)
    expect(screen.getAllByRole('button', { name: /save skill/i }).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /save instruction/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /new instruction/i })).toBeNull()
    expect(screen.queryByRole('button', { name: /new skill/i })).toBeNull()
  })

  test('fills a skill draft from a common starting point', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const templates = screen.getByRole('group', { name: /guidance templates/i })
    expect(
      screen.getByText(
        'This name appears when choosing guidance for a task. Use words a teammate would recognize.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/Use a short name people can recognize later/i)).toBeNull()
    await user.click(within(templates).getByRole('button', { name: /release notes/i }))

    expect(within(templates).getByRole('button', { name: /release notes/i })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    expect(screen.getByLabelText(/^guidance name$/i)).toHaveValue('release-notes')
    expect(screen.getByLabelText(/^short description$/i)).toHaveValue(
      'Draft release notes from accepted work'
    )
    expect(screen.getByLabelText(/^matching words for future tasks$/i)).toHaveValue('release')
    const instructions = screen.getByLabelText(/^steps for the agent$/i) as HTMLTextAreaElement
    expect(instructions.value).toContain('Group user-facing updates')
    expect(instructions.value).toContain('before release')
    expect(instructions.value).not.toContain('before publishing')
  })

  test('uses plain wording in the result check starter instruction', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const templates = screen.getByRole('group', { name: /guidance templates/i })
    await user.click(within(templates).getByRole('button', { name: /result check/i }))

    expect(within(templates).queryByRole('button', { name: /review checklist/i })).toBeNull()
    expect(screen.getByLabelText(/^guidance name$/i)).toHaveValue('result-check')
    expect(screen.getByLabelText(/^short description$/i)).toHaveValue(
      'Check work before the team uses it'
    )
    expect(screen.getByLabelText(/^matching words for future tasks$/i)).toHaveValue(
      'check result, ready to use'
    )
    const instructions = screen.getByLabelText(/^steps for the agent$/i) as HTMLTextAreaElement
    expect(instructions.value).toContain('link the file or page you checked')
    expect(instructions.value).not.toContain('link evidence')
  })

  test('offers a work status instruction that avoids repeated waiting', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const templates = screen.getByRole('group', { name: /guidance templates/i })
    await user.click(within(templates).getByRole('button', { name: /check work status/i }))

    expect(screen.getByLabelText(/^guidance name$/i)).toHaveValue('work-status-check')
    expect(screen.getByLabelText(/^short description$/i)).toHaveValue(
      'Summarize result and check status without repeated waiting'
    )
    expect(screen.getByLabelText(/^matching words for future tasks$/i)).toHaveValue(
      'check status, ready to finish, waiting checks'
    )
    const instructions = screen.getByLabelText(/^steps for the agent$/i) as HTMLTextAreaElement
    expect(instructions.value).toContain('Create one fresh status check')
    expect(instructions.value).toContain('reuse it instead of refreshing')
    expect(instructions.value).toContain('Needs a fix, Waiting, or Done')
    expect(instructions.value).toContain('open only the failed check or item')
    expect(instructions.value).toContain('do not keep checking in chat')
    expect(instructions.value).toContain('when one later check is useful')
    expect(instructions.value).toContain('project background watcher')
    expect(instructions.value).toContain('ready for the team to use')
    expect(instructions.value).not.toContain('review page')
    expect(instructions.value).not.toContain('review item')
    expect(instructions.value).not.toContain('ready for handoff')
    expect(instructions.value).not.toContain('merge readiness')
    expect(instructions.value).not.toContain('PR')
    expect(instructions.value).not.toContain('CI')
    expect(instructions.value).not.toContain('build status')
    expect(instructions.value).not.toContain('ACTION')
    expect(instructions.value).not.toContain('GitHub or GitLab')
    expect(instructions.value).not.toContain('npm run')
  })

  test('closes the create instruction draft without saving', async () => {
    const user = userEvent.setup()
    render(<SkillsView />)

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save reusable guidance/i })

    expect(within(dialog).getByRole('button', { name: 'Close without saving' })).toBeDefined()
    expect(within(dialog).queryByRole('button', { name: /^Cancel$/ })).toBeNull()

    await user.click(within(dialog).getByRole('button', { name: 'Close without saving' }))

    expect(screen.queryByRole('dialog', { name: /save reusable guidance/i })).toBeNull()
    expect(
      fetchMock.mock.calls.some(
        ([url, init]) =>
          url === '/api/v1/skills' && (init as RequestInit | undefined)?.method === 'POST'
      )
    ).toBe(false)
  })

  test('shows empty state after load with no skills', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    expect(screen.getByText(/checking work before sharing it/i)).toBeDefined()
    expect(screen.getByText(/writing a short update/i)).toBeDefined()
    expect(screen.queryByText(/create your first saved instruction/i)).toBeNull()
    expect(screen.queryByText(/review checklists/i)).toBeNull()
    expect(screen.queryByText(/release-note rules/i)).toBeNull()
  })

  test('guides empty saved guidance search toward clearing or creating', async () => {
    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })
    fireEvent.change(screen.getByLabelText(/search skills/i), {
      target: { value: 'release handoff' },
    })

    expect(screen.getByText('No skills match that search yet')).toBeDefined()
    expect(screen.getByText(/choose save skill and add it now/i)).toBeDefined()
    expect(screen.queryByText('No saved instruction matches that search yet')).toBeNull()
    expect(screen.queryByText('No saved instructions match your search')).toBeNull()
  })

  test('guides hidden saved guidance back to the full list', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-release',
            name: 'release-draft',
            description: 'Draft release notes',
            trigger_pattern: 'release',
            content: 'Summarize accepted work',
            enabled: true,
          },
        ],
      }),
    })

    render(<SkillsView />)

    await screen.findByText('release-draft')
    fireEvent.change(screen.getByLabelText(/search skills/i), {
      target: { value: 'handoff checklist' },
    })

    const emptyState = screen.getByTestId('saved-instructions-empty-state')
    expect(screen.getByText('Nothing matches your skills search.')).toBeDefined()
    expect(emptyState).toHaveTextContent('Nothing matches your skills search')
    expect(emptyState).toHaveTextContent('Use Show all skills to return to the full list.')
    expect(emptyState).not.toHaveTextContent('Nothing matches your saved instruction search')
    expect(emptyState).not.toHaveTextContent('Clear search to see saved instructions')

    fireEvent.click(within(emptyState).getByRole('button', { name: 'Show all skills' }))

    expect(screen.getByLabelText(/search skills/i)).toHaveValue('')
    expect(screen.getByText('release-draft')).toBeDefined()
  })

  test('uses saved guidance wording when search and filters hide results', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: [
          {
            id: 'skill-release',
            name: 'release-draft',
            description: 'Draft release notes',
            trigger_pattern: 'release',
            content: 'Summarize accepted work',
            enabled: true,
          },
        ],
      }),
    })

    render(<SkillsView />)

    await screen.findByText('release-draft')
    fireEvent.click(
      screen.getByRole('button', {
        name: /show skills that are ready to use/i,
      })
    )
    fireEvent.change(screen.getByLabelText(/search skills/i), {
      target: { value: 'handoff checklist' },
    })

    const emptyState = screen.getByTestId('saved-instructions-empty-state')
    expect(screen.getByText('Nothing matches this skills view.')).toBeDefined()
    expect(emptyState).toHaveTextContent('Nothing matches this skills view')
    expect(emptyState).toHaveTextContent(
      'Use Show all skills before assuming nothing useful is saved.'
    )
    expect(emptyState).not.toHaveTextContent('Nothing matches this saved instruction view')
    expect(emptyState).not.toHaveTextContent('hiding')
    expect(emptyState).not.toHaveTextContent('library')
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
    expect(screen.getByText('Matching words: webui')).toBeDefined()
    expect(screen.queryByText('Suggested for tasks that mention: webui')).toBeNull()
    expect(screen.queryByText(/Use when task says/i)).toBeNull()
    expect(screen.getByText('Everyone')).toBeDefined()
    expect(screen.queryByText('Saved in Global saved instructions')).toBeNull()
    expect(screen.queryByText('Saved in Global skills')).toBeNull()
    expect(screen.queryByText(/^Source:/i)).toBeNull()
    expect(screen.queryByText(/Suggested for:/i)).toBeNull()
    expect(screen.getByText('1 skill')).toBeDefined()
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
    expect(screen.getByText('Skills')).toBeDefined()
    expect(screen.queryByText('Saved in saved instructions')).toBeNull()
    expect(screen.queryByText('Saved as a saved instruction')).toBeNull()
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
    expect(within(summary).getAllByText('Ready to use').length).toBeGreaterThan(0)
    expect(within(summary).getAllByText('Check before use').length).toBeGreaterThan(0)
    expect(within(summary).queryByText('Needs setup')).toBeNull()
    expect(within(summary).queryByText('Needs install')).toBeNull()
    expect(within(summary).getAllByText('For one work tool').length).toBeGreaterThan(0)
    expect(within(summary).getByText('Show skills')).toBeDefined()
    expect(within(summary).queryByText('Show saved guidance')).toBeNull()
    expect(within(summary).queryByText('Tool-specific')).toBeNull()
    expect(within(summary).queryByText('Reuse view')).toBeNull()
    expect(within(summary).queryByText(/C[L]I scoped/)).toBeNull()

    const filters = within(summary).getByRole('group', {
      name: /skills view choices/i,
    })
    expect(
      within(filters).getByRole('button', {
        name: /show skills to check before use, 1 matching skill/i,
      })
    ).toBeDefined()
    expect(
      within(filters).queryByRole('button', {
        name: /show skills that need install first/i,
      })
    ).toBeNull()
    fireEvent.click(
      within(filters).getByRole('button', {
        name: /show skills for one work tool, 1 matching skill/i,
      })
    )

    expect(screen.getByText('cli-review')).toBeDefined()
    expect(screen.queryByText('release-draft')).toBeNull()
  })

  test('creates a skill through the Rust API', async () => {
    const user = userEvent.setup()
    const createRequest = deferred<Response>()
    fetchMock
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ ok: true, data: [] }),
      })
      .mockReturnValueOnce(createRequest.promise)

    const createdSkillResponse = {
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
    } as Response

    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save reusable guidance/i })
    expect(within(dialog).getByText(/check before saving/i)).toBeDefined()
    expect(screen.getByText('Keep private details out')).toBeDefined()
    expect(screen.getByText(/leave out passwords, access keys/i)).toBeDefined()
    expect(screen.queryByText('Safe to share')).toBeNull()
    expect(screen.queryByText(/secret keys/i)).toBeNull()
    expect(screen.queryByText(/tokens/i)).toBeNull()
    expect(screen.getByText(/choose this guidance manually/i)).toBeDefined()
    expect(screen.getByText(/words people usually write/i)).toBeDefined()

    await user.type(screen.getByLabelText(/^guidance name$/i), 'frontend-review')
    await user.type(screen.getByLabelText(/^short description$/i), 'Review frontend flows')
    await user.type(screen.getByLabelText(/^matching words for future tasks$/i), 'frontend')
    await user.type(
      screen.getByLabelText(/^steps for the agent$/i),
      'Check UI states and regressions'
    )
    await user.click(within(dialog).getByRole('button', { name: /save skill/i }))

    expect(within(dialog).getByRole('button', { name: /saving skill/i })).toBeDisabled()
    expect(within(dialog).queryByRole('button', { name: /^Saving\.\.\.$/i })).toBeNull()

    createRequest.resolve(createdSkillResponse)
    await waitFor(() => {
      expect(screen.getByText('frontend-review')).toBeDefined()
    })
    const confirmation = screen.getByText(
      'Saved "frontend-review". Open it to check or reuse it on a task.'
    )
    expect(confirmation).toHaveAttribute('aria-live', 'polite')

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
    const scrollSpy = vi
      .spyOn(Element.prototype, 'scrollIntoView')
      .mockImplementation(() => undefined)
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ ok: true, data: [] }),
    })

    render(<SkillsView />)

    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save reusable guidance/i })
    expect(within(dialog).getByText(/safe enough for this team space/i)).toBeDefined()
    expect(within(dialog).queryByText(/safe enough for the workspace/i)).toBeNull()
    await user.click(within(dialog).getByRole('button', { name: /save skill/i }))

    const nameAlert = screen.getByRole('alert')
    expect(nameAlert).toHaveTextContent('Name this guidance before saving it.')
    expect(nameAlert).toHaveAttribute('aria-live', 'polite')
    expect(screen.getByLabelText(/^guidance name$/i)).toHaveFocus()
    expect(screen.getByLabelText(/^guidance name$/i)).toHaveAttribute('aria-invalid', 'true')
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(0))
    const callsAfterFirstSubmit = scrollSpy.mock.calls.length

    await user.click(within(dialog).getByRole('button', { name: /save skill/i }))
    await waitFor(() => expect(scrollSpy.mock.calls.length).toBeGreaterThan(callsAfterFirstSubmit))

    await user.type(screen.getByLabelText(/^guidance name$/i), 'frontend-review')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    await user.click(within(dialog).getByRole('button', { name: /save skill/i }))

    const stepsAlert = screen.getByRole('alert')
    expect(stepsAlert).toHaveTextContent('Add the steps the agent should follow before saving.')
    expect(stepsAlert).toHaveAttribute('aria-live', 'polite')
    expect(screen.getByLabelText(/^steps for the agent$/i)).toHaveFocus()
    expect(screen.getByLabelText(/^steps for the agent$/i)).toHaveAttribute('aria-invalid', 'true')
    expect(fetchMock).toHaveBeenCalledTimes(1)
    scrollSpy.mockRestore()
  })

  test('shows a recovery step when saved instructions fail to load', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 500,
      json: async () => ({ message: 'HTTP 500: database unavailable' }),
    })

    render(<SkillsView />)

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Open Skills again to load the list.')
    expect(alert).toHaveTextContent('Choose Check Skills again to load the list.')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
    expect(screen.getByRole('button', { name: /check skills again/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: /^retry$/i })).toBeNull()
  })

  test('turns raw saved-instruction load errors into retry guidance', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
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
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent('Skills need to load again.')
    expect(alert).toHaveTextContent('Choose Check Skills again to load the list.')
    expect(alert).not.toHaveTextContent('HTTP 500')
    expect(alert).not.toHaveTextContent('database unavailable')
  })

  test('hides backend details without status codes when saved instructions fail to load', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    act(() => {
      useSkillsStore.setState({
        skills: [],
        installedSkills: [],
        loading: false,
        error: 'database unavailable',
        searchQuery: '',
      })
    })

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Skills need to load again.')
    expect(alert).toHaveTextContent('Choose Check Skills again to load the list.')
    expect(alert).not.toHaveTextContent('database unavailable')
  })

  test('shows access recovery when saved instructions fail from a role error', async () => {
    render(<SkillsView />)
    await waitFor(() => {
      expect(screen.getByText(/create your first skill/i)).toBeDefined()
    })

    act(() => {
      useSkillsStore.setState({
        skills: [],
        installedSkills: [],
        loading: false,
        error: 'owner role required',
        searchQuery: '',
      })
    })

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('After an owner or admin updates your access')
    expect(alert).not.toHaveTextContent('owner role required')
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

    await user.click(screen.getAllByRole('button', { name: /save skill/i })[0])
    const dialog = screen.getByRole('dialog', { name: /save reusable guidance/i })
    await user.type(screen.getByLabelText(/^guidance name$/i), 'frontend-review')
    await user.type(
      screen.getByLabelText(/^steps for the agent$/i),
      'Check UI states and regressions'
    )
    await user.click(within(dialog).getByRole('button', { name: /save skill/i }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(
      'Ask an owner or admin to update your Skills access for this team space, then choose Save skill again.'
    )
    expect(alert.textContent).not.toContain('workspace instructions')
    expect(alert.textContent).not.toContain('Code:')
    expect(alert.textContent).not.toContain('API 403')
    expect(alert.textContent).not.toContain('Forbidden')
  })

  test('shows loading state while fetching', () => {
    useSkillsStore.setState({ loading: true })
    render(<SkillsView />)
    expect(screen.getByText('Checking skills...')).toBeDefined()
  })
})
