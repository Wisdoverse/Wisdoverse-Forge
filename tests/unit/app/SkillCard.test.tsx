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
    expect(
      screen.getByText(/saved in team space saved instructions by platform team/i)
    ).toBeInTheDocument()
    expect(screen.queryByText(/workspace skills/i)).toBeNull()
    expect(screen.queryByText(/workspace saved instructions/i)).toBeNull()
    expect(screen.getByText('Matching words: release')).toBeInTheDocument()
    expect(screen.queryByText('Suggested for tasks that mention: release')).not.toBeInTheDocument()
    expect(screen.queryByText(/Use when task says/i)).toBeNull()
  })

  test('marks unavailable saved instructions as needing setup before use', () => {
    render(<SkillCard skill={{ ...baseSkill, installed: false }} onClick={() => {}} />)

    expect(screen.getByText('Needs setup before use')).toBeInTheDocument()
    expect(screen.queryByText('Install to use')).toBeNull()
    expect(
      screen.getByRole('button', {
        name: /release-review\. needs setup before use\. review release notes before publishing/i,
      })
    ).toBeInTheDocument()
  })

  test('guides users to details when a skill has no summary', () => {
    render(<SkillCard skill={{ ...baseSkill, description: '' }} onClick={() => {}} />)

    expect(
      screen.getByRole('button', {
        name: /release-review\. ready to reuse\. open saved instruction details to check the reusable instructions before using it/i,
      })
    ).toBeInTheDocument()
    expect(
      screen.getByText(
        'Open saved instruction details to check the reusable instructions before using it.'
      )
    ).toBeDefined()
    expect(screen.queryByText(/review the reusable instructions before using it/i)).toBeNull()
  })

  test('uses readable source fallback when saved-in metadata is missing', () => {
    render(
      <SkillCard skill={{ ...baseSkill, plugin: '   ', pluginAuthor: '   ' }} onClick={() => {}} />
    )

    expect(screen.getByText('Saved in saved instructions')).toBeInTheDocument()
    expect(screen.queryByText(/Saved in\s*$/)).toBeNull()
    expect(screen.queryByText(/by\s*$/)).toBeNull()
  })

  test('opens the selected skill', () => {
    const onClick = vi.fn()
    render(<SkillCard skill={baseSkill} onClick={onClick} />)

    fireEvent.click(screen.getByRole('button', { name: /release-review/i }))

    expect(onClick).toHaveBeenCalledWith(baseSkill)
  })
})
