import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import { TaskTemplatesSection } from '@app/pages/settings/ui/TaskTemplatesSection'

const getGroupsMock = vi.hoisted(() => vi.fn())

vi.mock('@app/entities/navigation/agent-group', () => ({
  agentGroupApi: { getGroups: getGroupsMock },
}))

const listTaskTemplatesMock = vi.hoisted(() => vi.fn())
const createTaskTemplateMock = vi.hoisted(() => vi.fn())
const deleteTaskTemplateMock = vi.hoisted(() => vi.fn())

vi.mock('@app/shared/api/orchestration', () => ({
  orchestrationApi: {
    listTaskTemplates: listTaskTemplatesMock,
    listRecurringTasks: vi.fn().mockResolvedValue([]),
    createTaskTemplate: createTaskTemplateMock,
    deleteTaskTemplate: deleteTaskTemplateMock,
  },
}))

function templateRow(overrides: Record<string, unknown> = {}) {
  return {
    id: 'tpl-1',
    name: 'Ship feature',
    title: 'Add one focused change',
    description: 'Do the thing',
    priority: 'high',
    requiresApproval: false,
    createdBy: 'user-1',
    createdAt: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

beforeEach(() => {
  listTaskTemplatesMock.mockReset()
  createTaskTemplateMock.mockReset()
  deleteTaskTemplateMock.mockReset()
  listTaskTemplatesMock.mockResolvedValue([])
})

afterEach(() => {
  cleanup()
  vi.clearAllMocks()
})

describe('TaskTemplatesSection', () => {
  test('explains the empty state and lists saved templates', async () => {
    listTaskTemplatesMock.mockResolvedValueOnce([])
    const { unmount } = render(<TaskTemplatesSection />)
    expect(
      await screen.findByText(/No saved templates yet. Create the first one above/)
    ).toBeDefined()

    unmount()
    listTaskTemplatesMock.mockResolvedValueOnce([
      templateRow({
        id: 'tpl-9',
        name: 'Release ship',
        priority: 'urgent',
        requiresApproval: true,
      }),
    ])
    render(<TaskTemplatesSection />)

    expect(await screen.findByText('Release ship')).toBeDefined()
    expect(screen.getByText('Add one focused change · waits for approval')).toBeDefined()
    expect(screen.getAllByText('Urgent').length).toBeGreaterThan(0)
  })

  test('saves a template with trimmed name and title', async () => {
    createTaskTemplateMock.mockResolvedValue(templateRow({ name: 'Release', title: 'Cut release' }))
    render(<TaskTemplatesSection />)
    await screen.findByText(/No saved templates yet/)

    fireEvent.change(screen.getByLabelText('Template name'), {
      target: { value: '  Release  ' },
    })
    fireEvent.change(screen.getByLabelText('Task title it writes'), {
      target: { value: ' Cut release ' },
    })
    fireEvent.change(screen.getByLabelText('Task brief it fills in'), {
      target: { value: 'Steps to ship' },
    })
    fireEvent.change(screen.getByLabelText('Priority'), { target: { value: 'urgent' } })
    fireEvent.click(screen.getByTestId('save-task-template'))

    await waitFor(() => expect(createTaskTemplateMock).toHaveBeenCalledTimes(1))
    expect(createTaskTemplateMock).toHaveBeenCalledWith({
      name: 'Release',
      title: 'Cut release',
      description: 'Steps to ship',
      priority: 'urgent',
      requiresApproval: false,
    })
    expect(await screen.findByText('Release')).toBeDefined()
    expect(screen.getByLabelText('Template name')).toHaveValue('')
  })

  test('requires a template name before saving', async () => {
    render(<TaskTemplatesSection />)
    await screen.findByText(/No saved templates yet/)

    fireEvent.click(screen.getByTestId('save-task-template'))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Give the template a short name so people can find it.'
    )
    expect(createTaskTemplateMock).not.toHaveBeenCalled()
  })

  test('confirms before removing and then deletes the template', async () => {
    listTaskTemplatesMock.mockResolvedValue([templateRow({ id: 'tpl-2' })])
    deleteTaskTemplateMock.mockResolvedValue(undefined)
    render(<TaskTemplatesSection />)

    const remove = await screen.findByTestId('delete-task-template-tpl-2')
    fireEvent.click(remove)
    expect(deleteTaskTemplateMock).not.toHaveBeenCalled()
    expect(screen.getByText('Confirm remove')).toBeDefined()

    fireEvent.click(screen.getByText('Confirm remove'))
    await waitFor(() => expect(deleteTaskTemplateMock).toHaveBeenCalledWith('tpl-2'))
    expect(await screen.findByText(/No saved templates yet/)).toBeDefined()
  })

  test('shows a retry when templates fail to load', async () => {
    listTaskTemplatesMock.mockRejectedValueOnce(new Error('down')).mockResolvedValueOnce([])
    render(<TaskTemplatesSection />)

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Open Task templates again in a moment. Forge could not load the saved templates.'
    )
    fireEvent.click(screen.getByRole('button', { name: /try again/i }))
    expect(await screen.findByText(/No saved templates yet/)).toBeDefined()
  })
})
