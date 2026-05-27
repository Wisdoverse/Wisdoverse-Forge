import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { AgentGroupSelector } from '@app/features/board/AgentGroupSelector'
import type { NavAgentGroup } from '@app/entities/agent-group'

afterEach(() => {
  cleanup()
})

const groups: NavAgentGroup[] = [{ id: 'lane-1', name: 'Delivery Lane', projectId: 'project-1' }]

describe('AgentGroupSelector', () => {
  test('explains that a project is needed before choosing a work lane', () => {
    render(
      <AgentGroupSelector
        groups={[]}
        selectedGroupId={null}
        selectedProjectId={null}
        onSelectGroup={vi.fn()}
      />
    )

    expect(screen.getByText('Work lane')).toBeDefined()
    const select = screen.getByRole('combobox', {
      name: /work lane for new tasks/i,
    }) as HTMLSelectElement

    expect(select.disabled).toBe(true)
    expect(select.title).toContain('Choose a project')
    expect(screen.getByRole('option', { name: /choose a project first/i })).toBeDefined()
  })

  test('explains that a work lane must be created before assigning tasks', () => {
    render(
      <AgentGroupSelector
        groups={[]}
        selectedGroupId={null}
        selectedProjectId="project-1"
        onSelectGroup={vi.fn()}
      />
    )

    const select = screen.getByRole('combobox', {
      name: /work lane for new tasks/i,
    }) as HTMLSelectElement

    expect(select.disabled).toBe(true)
    expect(select.title).toContain('Create a work lane')
    expect(screen.getByRole('option', { name: /create a work lane first/i })).toBeDefined()
  })

  test('selects the chosen work lane for new tasks', () => {
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
      name: /work lane for new tasks/i,
    }) as HTMLSelectElement

    expect(select.disabled).toBe(false)
    expect(select.title).toContain('where new tasks will go')

    fireEvent.change(select, { target: { value: 'lane-1' } })

    expect(onSelectGroup).toHaveBeenCalledWith('lane-1')
  })
})
