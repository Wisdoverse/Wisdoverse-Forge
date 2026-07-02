import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { SkillCard } from '@app/features/skills/SkillCard'
import type { Skill } from '@app/entities/skill'

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
    expect(screen.getByText(/saved for this team space by platform team/i)).toBeInTheDocument()
    expect(screen.queryByText(/saved in team space saved instructions/i)).toBeNull()
    expect(screen.queryByText(/workspace skills/i)).toBeNull()
    expect(screen.queryByText(/workspace saved instructions/i)).toBeNull()
    expect(screen.getByText('Matching words: release')).toBeInTheDocument()
    expect(screen.queryByText('Suggested for tasks that mention: release')).not.toBeInTheDocument()
    expect(screen.queryByText(/Use when task says/i)).toBeNull()
  })

  test('marks unavailable saved instructions as needing a check before use', () => {
    render(<SkillCard skill={{ ...baseSkill, installed: false }} onClick={() => {}} />)

    expect(screen.getByText('Check before use')).toBeInTheDocument()
    expect(screen.queryByText('Needs setup before use')).toBeNull()
    expect(screen.queryByText('Install to use')).toBeNull()
    expect(
      screen.getByRole('button', {
        name: /release-review\. check before use\. review release notes before publishing/i,
      })
    ).toBeInTheDocument()
  })

  test('guides users to details when a skill has no summary', () => {
    render(<SkillCard skill={{ ...baseSkill, description: '' }} onClick={() => {}} />)

    expect(
      screen.getByRole('button', {
        name: /release-review\. ready to reuse\. open details to check the reusable steps before using this saved instruction/i,
      })
    ).toBeInTheDocument()
    expect(
      screen.getByText(
        'Open details to check the reusable steps before using this saved instruction.'
      )
    ).toBeDefined()
    expect(
      screen.queryByText(
        'Open saved instruction details to check the reusable instructions before using it.'
      )
    ).toBeNull()
    expect(screen.queryByText(/review the reusable instructions before using it/i)).toBeNull()
  })

  test('uses readable source fallback when saved-in metadata is missing', () => {
    render(
      <SkillCard skill={{ ...baseSkill, plugin: '   ', pluginAuthor: '   ' }} onClick={() => {}} />
    )

    expect(screen.getByText('Saved as a saved instruction')).toBeInTheDocument()
    expect(screen.queryByText(/Saved in\s*$/)).toBeNull()
    expect(screen.queryByText(/by\s*$/)).toBeNull()
  })

  test('shows project-scoped saved instructions as saved for this project', () => {
    render(
      <SkillCard
        skill={{ ...baseSkill, plugin: 'Project saved instructions' }}
        onClick={() => {}}
      />
    )

    expect(screen.getByText(/saved for this project by platform team/i)).toBeInTheDocument()
    expect(screen.queryByText(/saved in project saved instructions/i)).toBeNull()
  })

  test('opens the selected skill', () => {
    const onClick = vi.fn()
    render(<SkillCard skill={baseSkill} onClick={onClick} />)

    fireEvent.click(screen.getByRole('button', { name: /release-review/i }))

    expect(onClick).toHaveBeenCalledWith(baseSkill)
  })
})
