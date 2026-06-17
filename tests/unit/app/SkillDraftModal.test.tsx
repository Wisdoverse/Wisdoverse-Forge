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
  test('uses explicit close wording before publishing a saved instruction', async () => {
    const onClose = vi.fn()
    const user = userEvent.setup()

    render(
      <SkillDraftModal
        open
        task={completedTask}
        artifacts={[{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }]}
        onClose={onClose}
      />
    )

    expect(screen.getByRole('button', { name: 'Close without publishing' })).toBeDefined()
    expect(screen.queryByRole('button', { name: /^Cancel$/ })).toBeNull()

    await user.click(screen.getByRole('button', { name: 'Close without publishing' }))

    expect(onClose).toHaveBeenCalledTimes(1)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  test('keeps the user in flow after publishing a saved instruction', async () => {
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
    expect(screen.getByText(/remove secret keys/i)).toBeDefined()
    expect(screen.queryByText(/tokens/i)).toBeNull()
    expect(screen.getByText(/choose the agents that should follow it/i)).toBeDefined()

    await userEvent.setup().click(screen.getByRole('button', { name: /publish instruction/i }))

    expect(await screen.findByTestId('skill-published-state')).toBeDefined()
    expect(screen.getByText('Instruction published')).toBeDefined()
    expect(screen.getByText('refactor-database-migration')).toBeDefined()

    const openSkills = screen.getByRole('link', { name: /open saved instructions/i })
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
    expect(document.querySelectorAll('[id="skill-draft-trigger-help"]')).toHaveLength(1)
    expect(document.querySelectorAll('[id="skill-draft-trigger-intro"]')).toHaveLength(1)
    expect(screen.getByLabelText(/^use when$/i)).toHaveAttribute(
      'aria-describedby',
      'skill-draft-trigger-intro skill-draft-trigger-help'
    )

    await user.clear(screen.getByLabelText(/^instruction name$/i))
    await user.click(screen.getByRole('button', { name: /publish instruction/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Name this instruction before publishing it.'
    )
    expect(screen.getByLabelText(/^instruction name$/i)).toHaveFocus()
    expect(screen.getByLabelText(/^instruction name$/i)).toHaveAttribute('aria-invalid', 'true')

    await user.type(screen.getByLabelText(/^instruction name$/i), 'migration-review')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByLabelText(/^instruction name$/i)).not.toHaveAttribute('aria-invalid', 'true')

    await user.clear(screen.getByLabelText(/^reusable instructions$/i))
    await user.click(screen.getByRole('button', { name: /publish instruction/i }))

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Keep or rewrite the reusable instructions before publishing.'
    )
    expect(screen.getByLabelText(/^reusable instructions$/i)).toHaveFocus()
    expect(screen.getByLabelText(/^reusable instructions$/i)).toHaveAttribute(
      'aria-invalid',
      'true'
    )
    expect(fetchMock).not.toHaveBeenCalled()
  })

  test('uses the task message for saved instruction defaults when the title is empty', () => {
    render(
      <SkillDraftModal
        open
        task={{
          ...completedTask,
          id: 'task-1234567890',
          params: {
            task: '',
            message: 'Check release readiness before launch\nInclude validation notes.',
          },
        }}
        artifacts={[]}
        onClose={() => {}}
      />
    )

    expect(screen.getByLabelText(/^instruction name$/i)).toHaveValue(
      'check-release-readiness-before-launch'
    )
    expect(screen.getByLabelText(/^use when$/i)).toHaveValue(
      'check release readiness before launch'
    )
    expect(
      (screen.getByLabelText(/^reusable instructions$/i) as HTMLTextAreaElement).value
    ).toContain('# Instruction: Check release readiness before launch')
    expect(screen.queryByDisplayValue(/task-1234567890/i)).toBeNull()
  })

  test('explains publish permission failures without raw API text', async () => {
    const user = userEvent.setup()
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      json: async () => ({ error: { message: 'Forbidden' } }),
    })

    render(
      <SkillDraftModal
        open
        task={completedTask}
        artifacts={[{ name: 'summary.md', mimeType: 'text/markdown', data: 'Done' }]}
        onClose={() => {}}
      />
    )

    await user.click(screen.getByRole('button', { name: /publish instruction/i }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Ask an owner or admin to let you create saved instructions, then publish again. Instruction was not published.'
    )
    expect(screen.queryByText(/HTTP 403/i)).toBeNull()
  })
})
