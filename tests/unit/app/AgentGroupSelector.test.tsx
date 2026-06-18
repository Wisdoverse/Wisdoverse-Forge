import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AgentGroupSelector } from '@app/features/board/AgentGroupSelector'
import type { NavAgentGroup } from '@app/entities/agent-group'

afterEach(() => {
  cleanup()
})

const groups: NavAgentGroup[] = [{ id: 'queue-1', name: 'Delivery Queue', projectId: 'project-1' }]

describe('AgentGroupSelector', () => {
  test('explains that a project is needed before choosing where tasks wait', () => {
    render(
      <AgentGroupSelector
        groups={[]}
        selectedGroupId={null}
        selectedProjectId={null}
        onSelectGroup={vi.fn()}
      />
    )

    expect(screen.getByText('Where tasks wait')).toBeDefined()
    const select = screen.getByRole('combobox', {
      name: /where new tasks wait/i,
    }) as HTMLSelectElement

    expect(select.disabled).toBe(true)
    expect(select.title).toBe('Choose a project before choosing where tasks wait.')
    expect(screen.getByRole('option', { name: /choose a project first/i })).toBeDefined()
    expect(screen.queryByText('Task queue')).toBeNull()
  })

  test('explains how to set up where tasks wait before sending work', () => {
    render(
      <AgentGroupSelector
        groups={[]}
        selectedGroupId={null}
        selectedProjectId="project-1"
        onSelectGroup={vi.fn()}
      />
    )

    const select = screen.getByRole('combobox', {
      name: /where new tasks wait/i,
    }) as HTMLSelectElement

    expect(select.disabled).toBe(true)
    expect(select.title).toBe('Set up where tasks wait, then come back here.')
    const previousActionPhrase = ['assigning', 'tasks'].join(' ')
    expect(select.title).not.toContain(previousActionPhrase)
    expect(screen.getByRole('option', { name: /set up where tasks wait first/i })).toBeDefined()
  })

  test('selects where new tasks wait', () => {
    const onSelectGroup = vi.fn()

    render(
      <AgentGroupSelector
        groups={groups}
        selectedGroupId={null}
        selectedProjectId="project-1"
        onSelectGroup={onSelectGroup}
      />
    )

    const select = screen.getByRole('combobox', {
      name: /where new tasks wait/i,
    }) as HTMLSelectElement

    expect(select.disabled).toBe(false)
    expect(select.title).toContain('where new tasks should wait')

    fireEvent.change(select, { target: { value: 'queue-1' } })

    expect(onSelectGroup).toHaveBeenCalledWith('queue-1')
  })
})
