import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SkillDraftModal } from '@app/features/detail/SkillDraftModal'
import { useSkillsStore } from '@app/shared/model/skills.store'
import type { TaskSummary } from '@app/shared/api/orchestration'

const fetchMock = vi.fn()
vi.stubGlobal('fetch', fetchMock)

const completedTask: TaskSummary = {
  id: 'task-1',
  groupId: 'group-1',
  state: 'completed',
  method: 'tasks/send',
  params: {
    task: 'Refactor database migration',
    message: 'Update the schema for v2',
  },
  priority: 'normal',
  progress: 100,
  createdAt: new Date(Date.now() - 7200000).toISOString(),
  updatedAt: new Date().toISOString(),
  completedAt: new Date().toISOString(),
}

afterEach(() => {
  cleanup()
  useSkillsStore.getState().reset()
  fetchMock.mockReset()
  localStorage.clear()
})

describe('SkillDraftModal', () => {
  test('keeps the user in flow after publishing a reusable skill', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        ok: true,
        data: {
          id: 'skill-1',
          organization_id: 'org-1',
          name: 'refactor-database-migration',
          description: 'Reusable database migration review',
          trigger_pattern: 'refactor database migration',
          content: 'Check migration safety and rollback notes.',
          enabled: true,
        },
      }),
    })

    render(
      <SkillDraftModal
        open
        task={completedTask}
        artifacts={[{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }]}
        onClose={() => {}}
      />
    )

    expect(screen.getByLabelText(/^use when$/i)).toBeDefined()
    expect(screen.getByText(/check before publishing/i)).toBeDefined()
    expect(screen.getByText('No secrets')).toBeDefined()
    expect(screen.getByText(/choose the agents that should use it/i)).toBeDefined()

    await userEvent.setup().click(screen.getByRole('button', { name: /publish skill/i }))

    expect(await screen.findByTestId('skill-published-state')).toBeDefined()
    expect(screen.getByText('Skill published')).toBeDefined()
    expect(screen.getByText('refactor-database-migration')).toBeDefined()

    const openSkills = screen.getByRole('link', { name: /open skills/i })
    const chooseAgent = screen.getByRole('link', { name: /choose agent/i })
    expect(openSkills.getAttribute('href')).toBe('/skills')
    expect(chooseAgent.getAttribute('href')).toBe('/agents')

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        '/api/v1/skills',
        expect.objectContaining({
          method: 'POST',
          body: expect.stringContaining('refactor-database-migration'),
        })
      )
    })
  })

  test('guides the user through required draft fields before publishing', async () => {
    const user = userEvent.setup()

    render(
      <SkillDraftModal
        open
        task={completedTask}
        artifacts={[{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }]}
        onClose={() => {}}
      />
    )

    expect(screen.getByText(/check 3 things before publishing/i)).toBeDefined()

    await user.clear(screen.getByLabelText(/^skill name$/i))
    await user.click(screen.getByRole('button', { name: /publish skill/i }))

    expect(screen.getByRole('alert')).toHaveTextContent('Name this skill before publishing it.')
    expect(screen.getByLabelText(/^skill name$/i)).toHaveFocus()

    await user.type(screen.getByLabelText(/^skill name$/i), 'migration-review')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    await user.clear(screen.getByLabelText(/^reusable instructions$/i))
    await user.click(screen.getByRole('button', { name: /publish skill/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Keep or rewrite the reusable instructions before publishing.'
    )
    expect(screen.getByLabelText(/^reusable instructions$/i)).toHaveFocus()
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
