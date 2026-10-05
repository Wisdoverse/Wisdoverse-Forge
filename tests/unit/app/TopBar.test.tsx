import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import '@app/i18n'
import { TopBar } from '@app/layouts/TopBar'
import type { ViewMode } from '@app/shared/model/board.types'

afterEach(cleanup)

function renderTaskTopBar(viewMode: ViewMode = 'board') {
  const onViewChange = vi.fn()
  const onCreateTask = vi.fn()
  const createTaskLabel =
    'Choose a project, set up an agent, and confirm its work location before you create this task.'

  render(
    <TopBar
      title="Tasks"
      showTaskControls
      viewMode={viewMode}
      onViewChange={onViewChange}
      onCreateTask={onCreateTask}
      createTaskLabel={createTaskLabel}
      onCmdK={vi.fn()}
    />
  )

  return { onViewChange, onCreateTask, createTaskLabel }
}

describe('TopBar task controls', () => {
  it('names the desktop view group and exposes its selected button state', () => {
    const { onViewChange } = renderTaskTopBar('board')
    const group = screen.getByRole('group', { name: 'Task view' })
    const boardButton = within(group).getByRole('button', { name: 'Board' })
    const listButton = within(group).getByRole('button', { name: 'List' })

    expect(boardButton).toHaveAttribute('aria-pressed', 'true')
    expect(listButton).toHaveAttribute('aria-pressed', 'false')
    fireEvent.click(listButton)
    expect(onViewChange).toHaveBeenCalledWith('list')
  })

  it('uses a labeled native selector to change the task view', () => {
    const { onViewChange } = renderTaskTopBar()
    const selector = screen.getByRole('combobox', { name: 'Task view' })

    expect(selector).toHaveValue('board')
    fireEvent.change(selector, { target: { value: 'timeline' } })
    expect(onViewChange).toHaveBeenCalledWith('timeline')
  })

  it('keeps the complete setup-aware task action available', () => {
    const { createTaskLabel, onCreateTask } = renderTaskTopBar()
    const createButton = screen.getByRole('button', { name: createTaskLabel })

    expect(createButton).toHaveTextContent(createTaskLabel)
    fireEvent.click(createButton)
    expect(onCreateTask).toHaveBeenCalledOnce()
  })
})
