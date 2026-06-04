import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { SkillCard } from '@app/features/skills/SkillCard'
import type { Skill } from '@app/shared/model/skills.store'

afterEach(cleanup)

const baseSkill: Skill = {
  id: 'skill-1',
  name: 'release-review',
  description: 'Review release notes before publishing',
  plugin: 'Workspace skills',
  pluginAuthor: 'Platform team',
  content: 'Check release notes',
  path: 'release-review',
  installed: true,
  marketplace: 'workspace',
  cliTool: '',
  triggerPattern: 'release',
}

describe('SkillCard', () => {
  test('shows readiness and source labels in plain language', () => {
    render(<SkillCard skill={baseSkill} onClick={() => {}} />)

    expect(
      screen.getByRole('button', {
        name: /release-review\. ready to reuse\. review release notes before publishing/i,
      })
    ).toBeInTheDocument()
    expect(screen.getByText('Ready to reuse')).toBeInTheDocument()
    expect(screen.getByText(/^Source:/)).toBeInTheDocument()
    expect(screen.getByText('Workspace skills')).toBeInTheDocument()
    expect(screen.getByText(/by Platform team/i)).toBeInTheDocument()
    expect(screen.getByText('Use when: release')).toBeInTheDocument()
  })

  test('marks unavailable skills as needing installation before use', () => {
    render(<SkillCard skill={{ ...baseSkill, installed: false }} onClick={() => {}} />)

    expect(screen.getByText('Install to use')).toBeInTheDocument()
  })

  test('opens the selected skill', () => {
    const onClick = vi.fn()
    render(<SkillCard skill={baseSkill} onClick={onClick} />)

    fireEvent.click(screen.getByRole('button', { name: /release-review/i }))

    expect(onClick).toHaveBeenCalledWith(baseSkill)
  })
})
